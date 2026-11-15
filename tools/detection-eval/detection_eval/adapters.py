"""Adapters: one per model family, each turning a window of text into raw
spans with app label names (`fixtures.LABELS`), in character offsets of the
window. `run.py` runs them over the windows of every fixture.

Families:
- `gliner2`: the `gliner2` package; the app's schema, one pass per family
  (`fixtures.FAMILIES`), as the app runs GLiNER2-PII.
- `gliner`: the `gliner` package (GLiNER v1), all prompts in one pass,
  flat NER.
- `token`: a Transformers token classifier; argmax tags (BIO or BIOES),
  or the constrained Viterbi path where the model's reference decoder takes
  it, joined into spans, score the mean tag probability.
- `pii_tracer`: perplexity-ai/PII-Tracer's own `predict` (remote code,
  constrained Viterbi).

Every adapter keeps windows under its model's input limit through `fits`.
"""

from __future__ import annotations

import re
from typing import Protocol

from .fixtures import FAMILIES, Entity
from .models import Model


class Adapter(Protocol):
    model: Model

    def fits(self, text: str) -> bool: ...

    def predict(self, text: str) -> list[Entity]: ...


def load(model: Model, device: str) -> Adapter:
    match model.family:
        case "gliner2":
            return Gliner2Adapter(model, device)
        case "gliner":
            return GlinerAdapter(model, device)
        case "token":
            return TokenAdapter(model, device)
        case "pii_tracer":
            return PiiTracerAdapter(model, device)
    raise ValueError(f"unknown family {model.family}")


def load_onnx(model: Model, onnx_file: str, threads: int = 4) -> Adapter:
    """An ONNX build's adapter on ONNX Runtime's CPU provider."""
    from pathlib import Path

    match model.family:
        case "token":
            return TokenOnnxAdapter(model, onnx_file, threads)
        case "gliner":
            path = Path(onnx_file)
            return GlinerOnnxAdapter(model, str(path.parent), path.name, threads)
    raise ValueError(
        f"{model.id}: no ONNX scorer for family {model.family} yet (token and gliner only)"
    )


def _join_adjacent(
    text: str, entities: list[Entity], labels: set[str], kinds: list[str] | None = None
) -> list[Entity]:
    """Joins same-label spans separated only by spaces (an address also by a
    comma). With `kinds` (each entity's own tag type), only parts of
    different types join, a given name and a surname, say: two spans the
    model tagged apart with the same type stay apart."""
    order = sorted(range(len(entities)), key=lambda i: (entities[i].start, entities[i].end))
    out: list[Entity] = []
    last_kinds: list[set] = []
    for i in order:
        e = entities[i]
        kind = kinds[i] if kinds is not None else None
        if out and e.label in labels and out[-1].label == e.label:
            gap = text[out[-1].end : e.start]
            joinable = gap.strip() == "" or (e.label == "address" and gap.strip() == ",")
            if kind is not None and kind in last_kinds[-1]:
                joinable = False
            if joinable and "\n" not in gap and e.start >= out[-1].end:
                prev = out[-1]
                out[-1] = Entity(prev.start, e.end, prev.label, min(prev.score, e.score))
                last_kinds[-1].add(kind)
                continue
        out.append(e)
        last_kinds.append({kind})
    return out


class Gliner2Adapter:
    def __init__(self, model: Model, device: str):
        from gliner2 import GLiNER2

        self.model = model
        self.engine = GLiNER2.from_pretrained(model.repo, revision=model.revision)
        self.engine.to(device)
        self.engine.eval()
        self.tokenizer = self.engine.processor.tokenizer
        # Prompt label → app label, in the app's families.
        self.families = [
            [p for p in family if p in model.prompts] for family in FAMILIES
        ]
        self.families = [f for f in self.families if f]

    def fits(self, text: str) -> bool:
        # Schema tokens plus text tokens within 512, as the app checks; the
        # largest family's prompt costs about 40 tokens.
        return len(self.tokenizer(text, add_special_tokens=False)["input_ids"]) <= 460

    def predict(self, text: str) -> list[Entity]:
        out = []
        for family in self.families:
            result = self.engine.extract_entities(
                text,
                family,
                threshold=self.model.cut,
                include_confidence=True,
                include_spans=True,
            )
            for label, spans in result.get("entities", {}).items():
                for span in spans:
                    out.append(
                        Entity(
                            span["start"],
                            span["end"],
                            self.model.prompts[label],
                            float(span["confidence"]),
                        )
                    )
        return out


class GlinerAdapter:
    def __init__(self, model: Model, device: str):
        from gliner import GLiNER

        self.model = model
        self.engine = GLiNER.from_pretrained(model.repo, revision=model.revision)
        self.engine.to(device)
        self.engine.eval()
        self._setup()

    def _setup(self) -> None:
        model = self.model
        self.prompts = list(model.prompts)
        self.tokenizer = self.engine.data_processor.transformer_tokenizer
        self.max_words = int(getattr(self.engine.config, "max_len", 384))
        label_tokens = sum(
            len(self.tokenizer(p, add_special_tokens=False)["input_ids"]) + 1
            for p in self.prompts
        )
        self.budget = 512 - label_tokens - 8

    def fits(self, text: str) -> bool:
        words = len(re.findall(r"\w+(?:[-_]\w+)*|\S", text))
        tokens = len(self.tokenizer(text, add_special_tokens=False)["input_ids"])
        return words <= self.max_words and tokens <= self.budget

    def predict(self, text: str) -> list[Entity]:
        found = self.engine.predict_entities(
            text, self.prompts, threshold=self.model.cut, flat_ner=True
        )
        out = [
            Entity(e["start"], e["end"], self.model.prompts[e["label"]], float(e["score"]))
            for e in found
        ]
        if self.model.id == "nuner-zero":
            # NuNER Zero tags tokens: join a label's adjacent tokens (its card).
            out = _join_adjacent(text, out, {e.label for e in out})
        return out


class TokenAdapter:
    def __init__(self, model: Model, device: str):
        import torch
        from transformers import AutoModelForTokenClassification, AutoTokenizer

        self.torch = torch
        self.model = model
        self.device = device
        self.tokenizer = AutoTokenizer.from_pretrained(model.repo, revision=model.revision)
        self.engine = AutoModelForTokenClassification.from_pretrained(
            model.repo, revision=model.revision
        ).to(device)
        self.engine.eval()
        self.id2label = {int(k): v for k, v in self.engine.config.id2label.items()}
        self.limit = min(512, getattr(self.tokenizer, "model_max_length", 512) or 512)

    def fits(self, text: str) -> bool:
        return len(self.tokenizer(text)["input_ids"]) <= self.limit

    def predict(self, text: str) -> list[Entity]:
        torch = self.torch
        enc = self.tokenizer(
            text, return_offsets_mapping=True, return_tensors="pt", truncation=True,
            max_length=self.limit,
        )
        offsets = enc.pop("offset_mapping")[0].tolist()
        enc = {k: v.to(self.device) for k, v in enc.items()}
        with torch.no_grad():
            logits = self.engine(**enc).logits[0].float()
        probs = torch.softmax(logits, dim=-1).cpu().numpy()
        return decode_tags(
            text, offsets, probs, self.id2label, self.model.labels, self.model.cut,
            self.model.decoder,
        )


def _split_tag(tag: str) -> tuple[str, str]:
    if tag == "O":
        return "O", ""
    prefix, sep, kind = tag.partition("-")
    if not sep or prefix not in ("B", "I", "E", "S"):
        return "I", tag
    return prefix, kind


def viterbi_tags(probs, id2label) -> list[int]:
    """The best tag path under BIOES transition rules (constrained Viterbi
    with no transition biases, the reference decoder's default operating
    point): a span opens with B or S, continues with I and closes with E,
    all of one type, and the path ends outside a span."""
    import numpy as np

    n_tags = probs.shape[1]
    parts = [_split_tag(id2label[i]) for i in range(n_tags)]
    log = np.log(np.clip(probs, 1e-12, 1.0))
    opens = np.array([p in ("O", "B", "S") for p, _ in parts])  # after O, E, S or the start
    closes = np.array([p in ("O", "E", "S") for p, _ in parts])
    allowed = np.zeros((n_tags, n_tags), dtype=bool)
    for a, (pa, ka) in enumerate(parts):
        for b, (pb, kb) in enumerate(parts):
            if pa in ("O", "E", "S"):
                allowed[a, b] = pb in ("O", "B", "S")
            else:  # B or I: continue the same type
                allowed[a, b] = pb in ("I", "E") and ka == kb
    neg = -1e18
    penalty = np.where(allowed, 0.0, neg)
    score = np.where(opens, log[0], neg)
    back = []
    for t in range(1, len(log)):
        cand = score[:, None] + penalty
        back.append(cand.argmax(axis=0))
        score = cand.max(axis=0) + log[t]
    score = np.where(closes, score, neg)
    path = [int(score.argmax())]
    for pointers in reversed(back):
        path.append(int(pointers[path[-1]]))
    return path[::-1]


def decode_tags(
    text, offsets, probs, id2label, labels, cut: float = 0.0, decoder: str = "argmax"
) -> list[Entity]:
    """Tags (BIO or BIOES, or bare types; per-token argmax, or the
    constrained Viterbi path) joined into spans; a span's score is its
    tokens' mean probability of the tag chosen, and a span under `cut` is
    dropped (before adjacent parts join)."""
    import numpy as np

    keep = [i for i, (start, end) in enumerate(offsets) if start != end]  # no special tokens
    if not keep:
        return []
    offsets = [offsets[i] for i in keep]
    probs = probs[keep]
    best = viterbi_tags(probs, id2label) if decoder == "viterbi" else probs.argmax(axis=-1).tolist()
    conf = probs[np.arange(len(best)), best].tolist()
    spans: list[list] = []  # [start, end, type, [scores], closed]
    for (start, end), tag_id, score in zip(offsets, best, conf):
        prefix, kind = _split_tag(id2label[tag_id])
        if prefix == "O":
            if spans:
                spans[-1][4] = True
            continue
        open_ = spans and not spans[-1][4] and spans[-1][2] == kind
        if prefix in ("B", "S") or not open_:
            spans.append([start, end, kind, [score], prefix in ("S", "E")])
        else:
            spans[-1][1] = end
            spans[-1][3].append(score)
            if prefix == "E":
                spans[-1][4] = True
    out = []
    kinds = []
    for start, end, kind, scores, _ in spans:
        label = labels.get(kind)
        if label is None:
            continue
        # Trim the leading space a SentencePiece or BPE offset carries.
        while start < end and text[start].isspace():
            start += 1
        score = sum(scores) / len(scores)
        if start < end and score >= cut:
            out.append(Entity(start, end, label, score))
            kinds.append(kind)
    return _join_adjacent(text, out, {"person", "address"}, kinds)


class TokenOnnxAdapter:
    """A token classifier's ONNX build on ONNX Runtime's CPU provider, with
    the PyTorch adapter's tokenizer and decoding (parity, timing)."""

    def __init__(self, model: Model, onnx_file: str, threads: int = 4):
        import onnxruntime as ort
        from transformers import AutoConfig, AutoTokenizer

        self.model = model
        self.tokenizer = AutoTokenizer.from_pretrained(model.repo, revision=model.revision)
        config = AutoConfig.from_pretrained(model.repo, revision=model.revision)
        self.id2label = {int(k): v for k, v in config.id2label.items()}
        self.limit = min(512, getattr(self.tokenizer, "model_max_length", 512) or 512)
        options = ort.SessionOptions()
        options.intra_op_num_threads = threads
        options.inter_op_num_threads = 1
        self.session = ort.InferenceSession(onnx_file, options, providers=["CPUExecutionProvider"])
        self.inputs = {i.name for i in self.session.get_inputs()}

    def fits(self, text: str) -> bool:
        return len(self.tokenizer(text)["input_ids"]) <= self.limit

    def predict(self, text: str) -> list[Entity]:
        import numpy as np

        enc = self.tokenizer(
            text, return_offsets_mapping=True, return_tensors="np", truncation=True,
            max_length=self.limit,
        )
        offsets = enc.pop("offset_mapping")[0].tolist()
        feed = {k: v.astype(np.int64) for k, v in enc.items() if k in self.inputs}
        logits = self.session.run(None, feed)[0][0].astype(np.float32)
        logits -= logits.max(axis=-1, keepdims=True)
        probs = np.exp(logits)
        probs /= probs.sum(axis=-1, keepdims=True)
        return decode_tags(
            text, offsets, probs, self.id2label, self.model.labels, self.model.cut,
            self.model.decoder,
        )


class GlinerOnnxAdapter(GlinerAdapter):
    """A GLiNER v1 ONNX build through the `gliner` package's ONNX path."""

    def __init__(self, model: Model, onnx_dir: str, onnx_file: str, threads: int = 4):
        import onnxruntime as ort
        from gliner import GLiNER

        options = ort.SessionOptions()
        options.intra_op_num_threads = threads
        options.inter_op_num_threads = 1
        self.model = model
        self.engine = GLiNER.from_pretrained(
            onnx_dir, load_onnx_model=True, load_tokenizer=True, onnx_model_file=onnx_file,
            session_options=options,
        )
        self._setup()


class PiiTracerAdapter:
    def __init__(self, model: Model, device: str):
        from transformers import AutoModel

        self.model = model
        self.engine = AutoModel.from_pretrained(
            model.repo, revision=model.revision, trust_remote_code=True
        ).to(device)
        self.engine.eval()
        from transformers import AutoTokenizer

        self.tokenizer = AutoTokenizer.from_pretrained(model.repo, revision=model.revision)

    def fits(self, text: str) -> bool:
        return len(self.tokenizer(text, add_special_tokens=False)["input_ids"]) <= 4000

    def predict(self, text: str) -> list[Entity]:
        spans, _sensitivity = self.engine.predict(text)
        out = []
        for s in spans:
            label = self.model.labels.get(s.label)
            score = float(getattr(s, "score", 1.0) or 1.0)
            if label is not None and score >= self.model.cut:
                out.append(Entity(s.start, s.end, label, score))
        return out
