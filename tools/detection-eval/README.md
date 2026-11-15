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
`--device cpu` runs without CUDA. `combo a+b` merges any members already run,
ranked in the order named. The report prints recall and precision per
language and set, English pooled over the holdout and real set, English
recall per category, and each option's decision-7 verdict against
`default`.

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
- **Threshold.** 0.5 for every model (the app's default), recorded in each
  snapshot.
- **Combos** (decision 11, [combo.py](detection_eval/combo.py)): the union of
  member snapshots. Overlapping spans of different members merge into the
  longest, with the higher-ranked member's category. GLiNER2-PII takes part
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
(`onnxruntime==1.27.0`, the app's version). A GLiNER2 build runs in the
app's own engine instead: point `MDOC_PII_MODEL_ROOT` at it and use
`run default`.
