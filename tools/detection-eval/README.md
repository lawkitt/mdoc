# Detection evaluation harness

Measures detection models and combos through mdoc's own correction
pipeline, for the detection-options run (lawkitt/mdoc#903). Not shipped;
nothing here changes the app.

Each model writes one `<id>.model.json` snapshot per document in the format
`just recognize` reads, with its labels renamed to the app's GLiNER2 label
names (`FAMILIES` in
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
python -m detection_eval.combo                  # the combos in models.py, from the snapshots above
python -m detection_eval.report --markdown report.md
python -m unittest discover tests               # self-tests; no model
```

The verdict rule (what "listed" means) is in `report.py`.

## Where to look for new candidates

Every rerun on new releases starts with this search.

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
language.

## Add a model

1. Add a `Model` to `MODELS` in [models.py](detection_eval/models.py), pinned
   to a full commit sha, with its licence and label mapping. Labels with no
   app label are left out, as the pipeline would drop them; a new category
   is a maintainer decision.
2. A new architecture family gets an adapter class in
   [adapters.py](detection_eval/adapters.py): `fits(text)` (the model's input
   limit) and `predict(text)` (spans in character offsets of the window).
3. `python -m detection_eval.run <id>`, then the report.

## What the numbers mean

- **Harness windows.** Every model starts from the same window plan, after
  the app's `windows.rs`, and each adapter halves a window until its `fits`
  accepts it, so two models can see different context. The app also decodes character references and rewrites
  `L.L.C.` for GLiNER2's word budget; the harness does not, so the
  `gliner2-pii` row (PyTorch, harness windows) beside `default` (the app)
  shows how much the harness itself moves the scores.
- **No score cut on token classifiers** or PII-Tracer: they keep the argmax
  or constrained-Viterbi tags their cards report, because a 0.5 cut on a
  mean softmax over dozens of tags measures the cut, not the model. Span
  models (GLiNER2, GLiNER) cut at 0.5, the app's threshold.
- **Complete runs only.** A verdict needs complete holdout and real-set runs
  of the option, the default and, for a combo, every member.

## ONNX builds

```sh
python -m detection_eval.export_onnx <id>                  # our FP32 export + INT8
python -m detection_eval.export_onnx <id> --quantize-only <model.onnx>
python -m detection_eval.run <id> --onnx <model.onnx> --as <id>.onnx-int8   # parity re-score (CPU)
python -m detection_eval.time_onnx <id> --onnx <model.onnx> --lang en       # s per 10,000 chars, peak memory
```

Prefer an upstream build that matches the weights (`UPSTREAM` in
[export_onnx.py](detection_eval/export_onnx.py)); list a build only if its
re-score matches its weights. Files stay local; nothing is uploaded.

Parity and timing cover the `token` and `gliner` families. GLiNER2 and
PII-Tracer builds have no ONNX scorer here: `export_onnx` writes them, but
parity and timing need an adapter that runs the build with the reference
decoding (the app's engine only loads its own pinned build).
