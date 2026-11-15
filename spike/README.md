# Burn and Core ML spike (lawkitt/mdoc#903, decision 29)

Branch-only spike code; never merged. Results are on #903.

- `gliner2-burn/`: burn-onnx 0.22 imports of the installed GLiNER2-PII FP32 fragments (build.rs reads them from the app data) and the span chain on Burn.
- `gliner2-rs/`: gliner2-rs 0.9.6, patched in through `[patch.crates-io]`: `GLINER2_BURN=flex|cpu|metal` runs the chain on Burn; `GLINER2_COREML=format=…,units=…,cache=…`, `GLINER2_EP_ONLY=encoder`, `GLINER2_ENCODER_OVERRIDE=<onnx>` and `GLINER2_PAD=512` for the Core ML variants.
- `bench/`: `recognize` (mdoc's example, `SPIKE_MODEL=fp16|fp32`) and `timing <text> [runs]`.
- `parity.sh <name> <env…>`: refreshes the public fixtures' snapshots in a scratch copy; `compare.py <base> <other>` diffs two copies.
- `make_text.py`: a ~10,000-character text from the public fixtures.
- `ios-ortbench/`: an iPhone app timing one ONNX model on ORT 1.27, CPU vs Core ML provider, with peak footprint.

Build: `cargo build --release -p bench` here (`--features fusion` with `CARGO_TARGET_DIR=target-fusion` for CubeCL fusion and autotune).
