# Detection evaluation harness

Measures detection models and combos through mdoc's own correction pipeline,
for the detection-options run (lawkitt/mdoc#903, decisions 7, 9, 11, 12, 23
and 24). Not shipped; nothing here changes the app.

Each model runs over a fixture set and writes one `<id>.model.json` snapshot
per document in the format `just recognize` reads, with its labels renamed to
the app's GLiNER2 label names (`FAMILIES` in
[crates/mdoc-detect/src/lib.rs](../../crates/mdoc-detect/src/lib.rs));
`just recognize --scoreboard <folder> --snapshot` then scores those
snapshots through the full pipeline (rule pack, corrections), exactly as it
scores the app's own.

## Setup

Python 3.12 with PyTorch (CUDA for the sweep; CPU works, slowly):

```sh
python -m venv .venv            # outside the repository is fine
.venv/bin/pip install "torch==2.11.0+cu128" --index-url https://download.pytorch.org/whl/cu128
.venv/bin/pip install -r tools/detection-eval/requirements.txt
```

Environment:

| Variable | What |
|---|---|
| `MDOC_HOLDOUT` | the private holdout folder (`lawkitt/mdoc-holdout`) |
| `MDOC_REAL_SET` | the real set's gold folder (`lawkitt/mdoc-probes`, `gold-r3`) |
| `MDOC_EVAL_OUT` | where runs go (default `~/.cache/mdoc-detection-eval`) |
| `HF_HOME` | the Hugging Face cache (weights download here) |

Sets: `fixtures` (the public recognition fixtures, always), `holdout` and
`real` (when their variables are set). The holdout and real set are private:
runs copy their documents under `MDOC_EVAL_OUT`, so keep it outside the
repository, and post only the report's aggregates.

Run every command from `tools/detection-eval/`.

## Rerun the report

```sh
python -m detection_eval.run default            # the app's detector (installed FP16), via just recognize
python -m detection_eval.run all --keep-going   # every model in models.py, on the GPU
python -m detection_eval.combo                  # decision 9's combos from the snapshots above
python -m detection_eval.report --markdown report.md
```

`run <id>...` runs some models; `--sets holdout,real` limits the sets;
`--device cpu` runs without CUDA. Relative `--out` paths are taken
from the current folder. `combo a+b` merges any members already run,
ranked in the order named. The report prints recall and precision per
language and set, English pooled over the holdout and real set, English
recall per category, and each option's decision-7 verdict against
`default`.

## Where to look

Every re-run on new releases (#903, decision 36) starts with a search for
new candidates (decision 50):

- The PIIMB leaderboard:
  [piimb/pii-masking-benchmark-leaderboard](https://huggingface.co/spaces/piimb/pii-masking-benchmark-leaderboard).
  Its synthetic English sets do not predict our real set, so treat it as a
  list of names, not a ranking.
- These Hugging Face authors' models: `fastino`, `jugaadsrl`,
  `knowledgator`, `bardsai`, `Horizon-Labs`, `OpenMed`.
- Hugging Face model searches for `gliner`, `pii`, `privacy`, `redact` and
  `anonymi`, each sorted by recently updated, downloads and likes.

Take only PII-oriented models: general extraction, chat, code, secrets and
clinical models stay out. Look at English models first, then multilingual
ones (English with Russian or EU languages), then models for one other
language (decision 38).

## Add a model

1. Add a `Model` to `MODELS` in [models.py](detection_eval/models.py): an
   id, the repository, its full revision (commit sha), the family, a size
   class, the licence and the label mapping.
   - A zero-shot model (`gliner2`, `gliner`) takes `prompts`: each label
     word it is asked for → an app label. Several words may map to one
     label.
   - A token classifier (`token`) takes `labels`: its tag names without
     the `B-`/`I-`/`E-`/`S-` prefix → an app label.
   - Labels with no app label are left out: the pipeline would drop them
     (decision 12). A new category is a maintainer decision.
2. A new architecture family gets an adapter class in
   [adapters.py](detection_eval/adapters.py): `fits(text)` (the model's input
   limit) and `predict(text)` (spans in character offsets of the window).
3. `python -m detection_eval.run <id>`, then the report.

A combo is an entry in `COMBOS` (members in rank order) or an ad-hoc
`combo a+b`.

## How a run works

- **Windows.** Every model reads the same windows: whole paragraphs packed
  up to 128 words, each heading starting a window, halved until the model's
  `fits` accepts them ([windows.py](detection_eval/windows.py), after the
  app's `windows.rs`). The app also decodes character references and
  rewrites `L.L.C.` for GLiNER2's word budget; the harness does not, so the
  `gliner2-pii` row (PyTorch, harness windows) beside `default` (the app)
  shows how much the harness itself moves the scores.
- **Score cut.** 0.5 (the app's threshold) for the span models, GLiNER2
  and GLiNER. Token classifiers and PII-Tracer keep their own decision, the
  argmax or constrained-Viterbi tags their cards report, with no score cut
  on top: a 0.5 cut on a mean softmax over dozens of tags measures the cut,
  not the model. Each snapshot records the cut applied (0 for none); set a
  `threshold` on a model to cut it.
- **Token spans.** Adjacent parts of a name or an address with different
  tag types (a given name and a surname) join into one span; spans a model
  tagged apart with the same type stay apart. OpenAI Privacy Filter decodes
  with its reference constrained BIOES Viterbi (default operating point).
- **Complete runs only.** Each set is written beside its folder and
  replaces it only when the run finishes (`meta.json` marks it), so a failed
  rerun leaves the previous run whole. The report and combos skip a set
  with no `meta.json`, and a verdict needs complete holdout and real-set
  runs of the option, the default and, for a combo, every member.
- **Combos** (decision 11, [combo.py](detection_eval/combo.py)): the union of
  member snapshots. A span that overlaps a higher-ranked member's span
  merges into it: the longer extent, the higher-ranked member's category.
  Each member's own overlapping spans are kept. GLiNER2-PII takes part
  as `default`, the app's own snapshots.

## Self-tests

```sh
python -m unittest discover tests   # the combo merge, sources, offsets, windows; no model
```

## ONNX builds

```sh
python -m detection_eval.export_onnx <id>                  # our FP32 export + INT8
python -m detection_eval.export_onnx <id> --quantize-only <model.onnx>
```

Prefer an upstream build that matches the weights (`UPSTREAM` in
[export_onnx.py](detection_eval/export_onnx.py)); a build is listed only if
its re-score matches its weights (decision 24). Files stay local; nothing
is uploaded.

```sh
python -m detection_eval.run <id> --onnx <model.onnx> --as <id>.onnx-int8   # parity re-score (CPU)
python -m detection_eval.time_onnx <id> --onnx <model.onnx> --lang en       # s per 10,000 chars, peak memory
```

Both run on ONNX Runtime's CPU provider with 4 threads, as the app does
(`onnxruntime==1.27.0`, the app's version), for the `token` and `gliner`
families. GLiNER2 and PII-Tracer builds have no ONNX scorer here yet:
`export_onnx` writes them, but parity and timing need an adapter that runs
the build with the reference decoding (the app's engine only loads its own
pinned build).
