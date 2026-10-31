# Android feasibility spike harness

A throwaway Android app for [ADR 0089](../../docs/adr/0089-android-app.md)'s
feasibility spike, the Android twin of [`tools/ios-spike`](../ios-spike/README.md).
It calls the core's `spike.run` through `mdoc-ffi`'s Kotlin bindings on
bundled copies of the fixtures and models, adds what only the platform knows
(device, Android version, RAM, thermal status each second) and writes
`spike-report.json` to the app's external files folder. It stays in the tree
until the Android rows of the
[report](../../docs/evidence/mobile/2026-10-31/README.md) are measured, then
leaves with them; the real app will be `apps/android`.

Peak memory is the process's resident high-water mark (`VmHWM`), against the
same 2 GiB budget; the thermal budget fails at status SEVERE or worse. An
emulator's row never counts.

## Run it

On the Windows PC, after `just android-setup`, with the GLiNER2 FP16 model set
up in the desktop mdoc (its folder is copied, never changed):

```sh
# A booted emulator or a phone on USB, by adb serial:
scripts/android-spike-device.sh --adb emulator-5554
# Firebase Test Lab phones (model:API):
scripts/android-spike-device.sh --test-lab blazer:36 shiba:36 a55xzh:36 bluejay:32
```

The script builds the core, stages the data (`scripts/android-spike-data.sh`:
both OCR bundles downloaded fresh, the model copied), builds the two APKs and
copies each report to `target/android-spike/reports/`.

A phone that is not on the PC can run it too: install
`app/build/outputs/apk/debug/app-debug.apk`, open **mdoc spike**, keep the
phone unplugged and cool, wait a few minutes, then **Share report**.

## What matters for the real app

- The core opens `libonnxruntime.so` and `libpdfium.so` by name
  (`linked_runtimes`); the app loads both with `System.loadLibrary` first.
- The models ship as assets here only because the harness needs no
  download; the app downloads them (ADR 0089).
