"""ONNX builds of a model: FP32 export and a dynamic INT8 build of it.

    python -m detection_eval.export_onnx <model-id> [--dest DIR] [--opset 18]
    python -m detection_eval.export_onnx <model-id> --quantize-only <model.onnx>

Decision 24 prefers an upstream or third-party build that matches the
weights; `UPSTREAM` lists the known ones, and `run --backend onnx` re-scores
any build for parity. Our own export, for a model without one:

- `token`: the token classifier, `input_ids` + `attention_mask` → `logits`,
  dynamic batch and sequence axes;
- `gliner`: the `gliner` package's own ONNX export (span model, all inputs
  the package's ONNX runtime path feeds);
- `pii_tracer`: the encoder and tag head, → per-token tag logits (the
  constrained Viterbi stays in the adapter);
- `gliner2`: not exported here: the app's engine (gliner2-rs) reads its
  eight-fragment layout; use an upstream build in that layout.

INT8 is ONNX Runtime's dynamic quantization of the FP32 file
(`QInt8` weights, per channel), written beside it as `model.int8.onnx`.
Files go to `<dest>/<model-id>/` with a `build.json` naming the source
revision and each file's sha256. Nothing is uploaded.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from .models import BY_ID, Model

# Known ONNX builds (decision 24): repository, revision, files.
UPSTREAM = {
    "gliner2-pii": ("jugaadsrl/gliner2-privacy-filter-PII-multi-onnx", "e594898629d452e8311796f5f329c7edbeda907c", "fp16_v2/, fp32/ (the app's)"),
    "bardsai-eu-pii-v2": ("bardsai/eu-pii-anonimization-multilang-v2", None, "onnx/model.onnx, onnx/model_quantized.onnx"),
    "nym-pii": ("Wismut/nym-pii-multilingual", None, "model.onnx, int8/model_int8.onnx"),
    "nym-pii-small": ("Wismut/nym-pii-multilingual-small", None, "model.onnx, int8/model_int8.onnx"),
    "gravitee-bert-small": ("gravitee-io/bert-small-pii-detection", None, "model.onnx, model.quant.onnx"),
    "openai-privacy-filter": ("openai/privacy-filter", None, "onnx/model.onnx, onnx/model_quantized.onnx"),
    "gliner-pii-edge": ("knowledgator/gliner-pii-edge-v1.0", None, "onnx/model.onnx, onnx/model_quint8.onnx"),
    "pii-tracer": ("lemonade-sdk/pplx-pii-masking-onnx", None, "model.onnx (third party)"),
}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def quantize(fp32: Path) -> Path:
    from onnxruntime.quantization import QuantType, quantize_dynamic

    int8 = fp32.with_name(fp32.stem + ".int8.onnx")
    quantize_dynamic(
        str(fp32),
        str(int8),
        weight_type=QuantType.QInt8,
        per_channel=True,
        use_external_data_format=fp32.stat().st_size > 1_900_000_000,
    )
    return int8


def export_token(model: Model, dest: Path, opset: int) -> Path:
    import torch
    from transformers import AutoModelForTokenClassification, AutoTokenizer

    tokenizer = AutoTokenizer.from_pretrained(model.repo, revision=model.revision)
    net = AutoModelForTokenClassification.from_pretrained(
        model.repo, revision=model.revision, torch_dtype=torch.float32
    ).eval()
    sample = tokenizer(["Jane Doe lives at 1 Main Street."], return_tensors="pt")
    path = dest / "model.onnx"

    class Logits(torch.nn.Module):
        def __init__(self, inner):
            super().__init__()
            self.inner = inner

        def forward(self, input_ids, attention_mask):
            return self.inner(input_ids=input_ids, attention_mask=attention_mask).logits

    torch.onnx.export(
        Logits(net),
        (sample["input_ids"], sample["attention_mask"]),
        str(path),
        input_names=["input_ids", "attention_mask"],
        output_names=["logits"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "sequence"},
            "attention_mask": {0: "batch", 1: "sequence"},
            "logits": {0: "batch", 1: "sequence"},
        },
        opset_version=opset,
        dynamo=False,
    )
    tokenizer.save_pretrained(dest)
    net.config.save_pretrained(dest)
    return path


def export_pii_tracer(model: Model, dest: Path, opset: int) -> Path:
    import torch
    from transformers import AutoModel, AutoTokenizer

    tokenizer = AutoTokenizer.from_pretrained(model.repo, revision=model.revision)
    net = AutoModel.from_pretrained(
        model.repo, revision=model.revision, trust_remote_code=True
    ).float().eval()
    sample = tokenizer(["Jane Doe lives at 1 Main Street."], return_tensors="pt", add_special_tokens=False)
    path = dest / "model.onnx"

    class Logits(torch.nn.Module):
        def __init__(self, inner):
            super().__init__()
            self.inner = inner

        def forward(self, input_ids, attention_mask):
            out = self.inner(input_ids, attention_mask)
            return out[0] if isinstance(out, (tuple, list)) else out.logits

    torch.onnx.export(
        Logits(net),
        (sample["input_ids"], sample["attention_mask"]),
        str(path),
        input_names=["input_ids", "attention_mask"],
        output_names=["logits"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "sequence"},
            "attention_mask": {0: "batch", 1: "sequence"},
            "logits": {0: "batch", 1: "sequence"},
        },
        opset_version=opset,
        dynamo=False,
    )
    tokenizer.save_pretrained(dest)
    return path


def export_gliner(model: Model, dest: Path, opset: int) -> Path:
    from gliner import GLiNER

    net = GLiNER.from_pretrained(model.repo, revision=model.revision)
    net.save_pretrained(dest)
    # The package's own exporter (gliner >= 0.2.22).
    result = net.export_to_onnx(str(dest), onnx_filename="model.onnx", opset=opset, quantize=False)
    path = Path(result.get("onnx_path", dest / "model.onnx")) if isinstance(result, dict) else dest / "model.onnx"
    return path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("model")
    parser.add_argument("--dest", default=str(Path.home() / ".cache" / "mdoc-detection-eval" / "onnx"))
    parser.add_argument("--opset", type=int, default=18)
    parser.add_argument("--quantize-only", help="an existing FP32 .onnx to quantize")
    args = parser.parse_args()
    model = BY_ID[args.model]
    dest = Path(args.dest) / model.id
    dest.mkdir(parents=True, exist_ok=True)
    if args.quantize_only:
        fp32 = Path(args.quantize_only)
    else:
        match model.family:
            case "token":
                fp32 = export_token(model, dest, args.opset)
            case "gliner":
                fp32 = export_gliner(model, dest, args.opset)
            case "pii_tracer":
                fp32 = export_pii_tracer(model, dest, args.opset)
            case _:
                upstream = UPSTREAM.get(model.id)
                raise SystemExit(
                    f"{model.id}: no export for family {model.family}; upstream build: {upstream}"
                )
    int8 = quantize(fp32)
    files = {
        p.name: {"bytes": p.stat().st_size, "sha256": sha256(p)}
        for p in sorted(dest.iterdir())
        if p.is_file() and p.suffix in (".onnx", ".data", ".json")
        and p.name != "build.json"
    }
    build = {
        "model": model.id,
        "repository": model.repo,
        "revision": model.revision,
        "fp32": fp32.name,
        "int8": int8.name,
        "opset": args.opset,
        "files": files,
    }
    (dest / "build.json").write_text(json.dumps(build, indent=2) + "\n", encoding="utf-8")
    print(f"{model.id}: {fp32} and {int8}")


if __name__ == "__main__":
    main()
