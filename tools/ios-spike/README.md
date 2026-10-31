# iOS feasibility spike harness

A throwaway SwiftUI app for ADR 0085's feasibility spike. It calls the core's
`spike.run` through `mdoc-ffi` on bundled copies of the fixtures and models,
adds what only the platform knows (device, iOS version, thermal state each
second, memory headroom) and writes `Documents/spike-report.json`. It stays in
the tree until the device rows of the
[report](../../docs/evidence/mobile/2026-10-31/README.md) are measured, then
leaves with them; the real app is `apps/ios`.

## Run it

On the Apple Silicon Mac, with both OCR bundles and the GLiNER2 FP16 model set
up in mdoc (their files are copied, never changed):

```sh
# A connected, paired phone with Developer Mode on:
DEVELOPMENT_TEAM=<team-id> scripts/ios-spike-device.sh
# A booted Simulator:
scripts/ios-spike-device.sh --simulator <udid>
```

The script builds the core (`just ios-core`), stages the data
(`scripts/ios-spike-data.sh`), builds and installs the app, waits for it to
finish and copies the report to `target/ios-spike/reports/`. Let the phone
cool to room temperature and unplug it from power before a device run; the
run takes a few minutes and the phone must stay unlocked.

## Build settings that matter

`MdocSpike/Spike.xcconfig` carries them; the real app needs the same:

- `-Wl,-exported_symbol,_OrtGetApiBase`, `STRIP_STYLE = non-global` and
  `ENABLE_DEBUG_DYLIB = NO`: ONNX Runtime is linked into the executable, and
  ort's loader looks `OrtGetApiBase` up in the image that holds the core.
- PDFium is `pdfium.xcframework`, embedded and signed. Its build (bblanchon
  7988) has a minimum iOS of 26.0, which is also the app's minimum
  (ADR 0085).
