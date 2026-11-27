# Local model runtime probes

Reports and logs from running each Settings model bundle once, offline, on
one machine. No test reads them. They hold synthetic input only: no
weights, replacement maps or private fixtures.

## What the files are

- [machine.json](machine.json): the machine, toolchain and build (an
  unoptimized debug test executable).
- [measurements.json](measurements.json): per-run elapsed time and peak
  RSS for each bundle, English and Russian.
- `<model>.json`: the probe's report (configuration, model revision and
  artifact SHA-256 digests, input SHA-256, timings, warnings and outputs).
- `<model>-setup.log` and `<model>-offline.log`: the two probes' output.

`<model>` is one of `ocr-cyrillic`, `ocr-v6`, `pii-fp16`, `pii-fp32`.

## Limits

- Elapsed time includes model checks, verification, loading and inference.
  Peak RSS is the process maximum across the English and Russian runs,
  from `/usr/bin/time -l`. Neither is a model memory limit or a release
  performance promise.
- Runtime success does not establish recognition quality. On the one-page
  [OCR fixtures](../ocr-qualification/README.md), Cyrillic matched both
  transcripts; V6 matched English but lost Russian words, a known language
  failure Settings shows.
- The pseudonymization input is a short synthetic example repeated four
  times per language. Both precisions returned the same spans and categories,
  with slightly different scores; FP32 was slower and used more memory.
  This smoke case does not establish recall or qualify either precision;
  the [qualification corpus](../../../tools/pseudonymization/README.md) is
  separate, and complete-document review stays required.
- Only macOS on Apple Silicon was probed.

## Reproduce

Compile the ignored probes, find the test executable Cargo prints, and
choose one model:

```sh
cargo test -p mdoc --bin mdoc --no-run
# Verified setup; may download the selected bundle:
env MDOC_MODEL=pii-fp32 <test-executable> document::comparison::tests::settings_model_setup_probe --exact --ignored --nocapture
# Offline inference on installed artifacts; writes only the named report:
env MDOC_MODEL=pii-fp32 MDOC_REPORT=/absolute/path/report.json /usr/bin/time -l sandbox-exec -p '(version 1)(allow default)(deny network*)' <test-executable> document::comparison::tests::settings_model_offline_probe --exact --ignored --nocapture
```

Setup reuses verified artifacts already installed; no inference path
downloads.
