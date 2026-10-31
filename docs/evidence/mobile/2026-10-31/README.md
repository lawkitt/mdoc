# Mobile feasibility spike, 2026-10-31

Measurements for [ADR 0085](../../../adr/0085-mobile-apps.md)'s feasibility
spike: GLiNER2 FP16 and PP-OCR through the shared Rust core
(`crates/mdoc-ffi`) on the iOS targets. Both device rows are measured: the
iPhone 17 Pro passes, the iPhone 12 Pro fails the 20-page scan budget, so the
detector floor is 8 GB (below). The Apple M4 Mac and iOS Simulator sections
are stand-ins that do not decide what ships.

## Budgets (ADR 0085)

GLiNER2 FP16 ships on a device class when it meets all of:

| Budget | Limit |
| --- | --- |
| Peak memory (`phys_footprint`) | ≤ 2 GiB |
| Cold model load | ≤ 10 s |
| A 20-page document scanned | ≤ 60 s |
| Thermal state | no serious throttling |

The floor was to be 6 GB if the iPhone 12 Pro passed every budget, otherwise
8 GB ([ADR 0088](../../../adr/0088-ios-on-real-devices.md)).

## Device classes

| Device | RAM | Result | Reports |
| --- | --- | --- | --- |
| iPhone 17 Pro (top) | 12 GB | **Pass** (memory 1.96–1.97 of 2 GiB, scan 35–43 s) | [run 1](iphone-17-pro.json), [run 2](iphone-17-pro-run2.json) |
| iPhone 12 Pro | 6 GB | **Fail**: the 20-page scan exceeded two minutes (budget 60 s) | [iphone-12-pro.json](iphone-12-pro.json) |

**Floor: 8 GB.** The iPhone 12 Pro meets the memory, load and thermal budgets
but scans about 3.5 times slower than the iPhone 17 Pro, so a 20-page document
takes about two minutes against the 60 s budget. INT8 was not tried, by the
maintainer's choice: it would need 2 to 2.3 times FP16's speed on that phone.
`DETECTOR_FLOOR_BYTES` (7 GiB) already sits between the 6 GB class (the 12 Pro
reports 5.58 GiB) and the 8 GB class (about 7.5 GiB), so it stays.

To measure a device, on the Apple Silicon Mac with both OCR bundles and the
GLiNER2 FP16 model set up in mdoc, the phone on iOS 26 or later, unlocked,
paired, in Developer Mode and cooled to room temperature:

```sh
DEVELOPMENT_TEAM=<team-id> scripts/ios-spike-device.sh
```

The script builds the core and the harness, installs it, waits for the run
(a few minutes) and prints the path of the JSON report it copied back
(`target/ios-spike/reports/device-<id>-<time>.json`). Run it once per phone
and hand the two reports to the hub, which fills the rows above.

The first run on a phone also registers it with the team and makes the
signing certificate. iOS then has to verify that certificate online before
the app may launch: tap the installed **mdoc spike** icon once with a working
connection to `ppq.apple.com` (a VPN can block it; a failed check is cached,
so delete the app, restart the phone and reinstall), then rerun the script.

## iPhone 17 Pro

`iPhone18,1`, 12 GB, iOS 27.0.1, `mobile` profile, ONNX Runtime 1.27.0 CPU,
PDFium 7988, both runs 2026-10-31 with the phone on its USB-C cable
(charging). Run 1: core `a41003f`, launched by hand on the phone after the
certificate check, at 100 %. Run 2: core `0ba7486`, launched by
`scripts/ios-spike-device.sh` three minutes after the iPhone 12 Pro run. Raw
reports: [run 1](iphone-17-pro.json), [run 2](iphone-17-pro-run2.json).

| Measurement | Run 1 | Run 2 | Budget |
| --- | ---: | ---: | --- |
| Cold load (runtime + GLiNER2 sessions) | 0.91 s | 0.75 s | ≤ 10 s |
| Seconds per 512-token window (median of 5) | 0.489 s | 0.433 s | |
| 20-page scan | 43.2 s | 35.0 s | ≤ 60 s |
| – windows / detections / inference | | 131 / 380 / 33.8 s | |
| Detector peak memory (`phys_footprint`) | 2,116,783,624 bytes: 1.97 GiB | 2,103,594,504 bytes: 1.96 GiB | ≤ 2 GiB |
| Memory available to the app at start | 3.29 GiB | 3.29 GiB | |
| Thermal state | nominal, then fair from 20 s | fair from the start | below serious |
| OCR `english.pdf` / `russian.pdf` | 0.46 s / 0.40 s, both exact | 0.43 s / 0.39 s, both exact | |

All four budgets pass in both runs. Load, scan time and heat have room to
spare; memory is 1.5–2 % under the limit, which the maintainer set as 2 GiB
after the peak fell between 2 GB and 2 GiB (ADR 0085 says GiB). The app may
use only about 3.3 GiB of the phone's 12 GB without the
increased-memory-limit entitlement.

## iPhone 12 Pro

`iPhone13,3`, 6 GB (reports 5,993,512,960 bytes: 5.58 GiB), iOS 27.2, core
`0ba7486` (`mobile` profile, ONNX Runtime 1.27.0 CPU, PDFium 7988), measured
2026-10-31 by `scripts/ios-spike-device.sh` with the phone on its USB-C cable
(charging) and unlocked, its thermal state nominal at the start. Raw report:
[iphone-12-pro.json](iphone-12-pro.json).

| Measurement | iPhone 12 Pro | Budget |
| --- | ---: | --- |
| Cold load (runtime + GLiNER2 sessions) | 1.75 s | ≤ 10 s |
| Seconds per 512-token window (median of 5) | 1.525 s (1.48–1.57) | |
| 20-page scan | **over 120 s**: stopped by the scan deadline | ≤ 60 s |
| Detector peak memory (`phys_footprint`) | 2,116,898,312 bytes: 1.97 GiB | ≤ 2 GiB |
| Memory available to the app at start | 2.99 GiB | |
| Thermal state | nominal, then fair from 80 s | below serious |
| OCR `english.pdf` / `russian.pdf` | 0.76 s / 0.73 s, both exact | |

The scan budget fails; the other three pass. ADR 0002's two-minute deadline
stopped the scan (`scan_error`: "Scan exceeded two minutes"), so the report
has no scan time. A window takes 3.5 times as long as on the iPhone 17 Pro
(1.525 s against 0.433 s on the same core), which puts the whole scan at
about 3.5 × 35.0 s ≈ 123 s, twice the budget. Memory fits: the peak is the
same 1.97 GiB as on the 17 Pro, and the app may use 2.99 GiB. Meeting 60 s
would need 2 to 2.3 times FP16's speed (the scan projected from 17 Pro run 2
or run 1: 123 s or 135 s), which the maintainer judged too far for INT8 to
be worth an export, a new pinned model and a qualification run; the floor
is 8 GB.

## Android (ADR 0089)

Measured 2026-10-31 on Firebase Test Lab phones through
[`tools/android-spike`](../../../../tools/android-spike/README.md): the same
`spike::run`, core `0.1.0` built by `scripts/build-android-core.sh` (ONNX
Runtime 1.27.0 CPU from Microsoft's Android AAR, PDFium 7988), the models
downloaded on the phone through the core's `Setup` (as the app does), four
intra-op threads as on iOS. Peak memory is the process's resident high-water
mark (`VmHWM`); the thermal budget fails at status SEVERE or worse
(ADR 0089). Run with
`scripts/android-spike-device.sh --test-lab blazer:36 shiba:36 a55xzh:36 bluejay:32`.

| Phone | RAM (reported) | Peak memory | Cold load | s per 512-token window | 20-page scan | Thermal | OCR | Report |
| --- | --- | ---: | ---: | ---: | --- | --- | --- | --- |
| Pixel 10 Pro (Tensor G5), Android 16 | 16 GB (15.2 GiB) | 1.68 GiB | 3.1 s | 1.76 | **Fail**: over 2 min (about 155 s est.) | none | both exact | [json](android-pixel-10-pro.json) |
| Galaxy A55 (Exynos 1480), Android 16 | 8 GB (7.3 GiB) | 1.65 GiB | 3.8 s | 2.01 | **Fail**: over 2 min (about 180 s est.) | none | both exact | [json](android-galaxy-a55.json) |
| Pixel 8 (Tensor G3), Android 16 | 8 GB (7.4 GiB) | 1.66 GiB | 3.9 s | 2.53 | **Fail**: over 2 min (about 225 s est.) | none | both exact | [json](android-pixel-8.json) |
| Pixel 6a (Tensor), Android 12 | 6 GB (5.5 GiB) | 1.62 GiB | 4.9 s | 4.55 | **Fail**: over 2 min (about 400 s est.) | none | both exact | [json](android-pixel-6a.json) |
| *Stand-in*: API 36 emulator, x86_64 | 6 GB | 1.81 GiB | 5.2 s | 1.58 | over 2 min | n/a | both exact | [json](android-emulator.json) |

The scan stops itself at two minutes, so its time is estimated from the
window time: the iPhone 17 Pro scanned the same 20 pages as 131 windows in
43 s at 0.49 s per probe window; each phone's estimate scales that by its
own window time. Peak memory covers the probe and two minutes of the scan.

**No Android phone meets the 60 s scan budget with GLiNER2 FP16 on ONNX
Runtime's CPU provider.** Memory, cold load, heat and OCR pass everywhere,
the 6 GB Pixel 6a included. A window takes 3.6 (Pixel 10 Pro) to 9 times
(Pixel 6a) as long as on the iPhone 17 Pro, with the same model, runtime
version and thread count, so the gap is in the Android CPU path rather than
the core. Android's floor constant is not set: the next step is the
maintainer's (see Findings).

## What is measured

[`spike::run`](../../../crates/mdoc-ffi/src/spike.rs) does the same on every
platform; the harness ([`tools/ios-spike`](../../../tools/ios-spike/README.md))
and the Mac stand-in (`crates/mdoc-ffi/examples/spike.rs`) both call it.

- **Document**: `legal-opinion.docx`, the scrubbed 37-page legal opinion,
  converted in-process through AnyDoc; its first 20/37 of words (9,589 words,
  63 KB of Markdown, cut at a line end) are the 20-page document.
- **Cold load**: loading ONNX Runtime into ort plus building the GLiNER2
  sessions, the first time in the process.
- **Seconds per 512-token window**: the longest whole-word prefix of the
  document that fits 512 schema+text tokens (502 tokens), median of five runs.
- **20-page scan**: `mdoc_detect::scan_at`, the desktop scan path (digest
  check, windows, engine load, inference, structured rules) with timings.
- **Peak memory**: `task_vm_info.ledger_phys_footprint_peak` after the scan,
  so the detector's peak (conversion included, OCR not yet run).
- **OCR**: `english.pdf` with PP-OCRv6 Small and `russian.pdf` with the
  Cyrillic bundle through `mdoc_import::import`, compared word for word with
  their `.txt` files (the desktop OCR smoke test's rule).
- **Thermal state** (harness only): `ProcessInfo.thermalState` every second;
  the Simulator reports the Mac's, so only a device row counts.

## Stand-in: Apple M4 Mac (macOS)

Apple M4 (4 performance and 6 efficiency cores, 24 GiB), macOS 15.8.1,
Rust 1.99.0, release profile, desktop runtimes (ONNX Runtime 1.27.0, PDFium
7988), commit `80bd711`. The machine was busy (other worktrees building;
load average 10–28), so these times are pessimistic for an M4, and an M4 is
far faster than either phone. Raw reports:
[run 1](mac-m4-run1.json), [run 2](mac-m4-run2.json),
[run 3](mac-m4-run3.json).

| Measurement | Run 1 | Run 2 | Run 3 | Budget |
| --- | ---: | ---: | ---: | --- |
| Cold load | 1.90 s | 0.78 s | 0.81 s | ≤ 10 s |
| – ONNX Runtime into ort | 1.01 s | 0.00 s | 0.00 s | |
| – GLiNER2 sessions | 0.89 s | 0.78 s | 0.81 s | |
| Seconds per 512-token window (median) | 0.343 s | 0.330 s | 0.332 s | |
| 20-page scan | 26.4 s | 26.3 s | 27.2 s | ≤ 60 s |
| – windows / mean tokens | 131 / 269 | 131 / 269 | 131 / 269 | |
| – inference | 24.9 s | | | |
| – digest check, engine load | 0.27 s, 0.87 s | | | |
| Detector peak memory | 1.41 GiB | 1.46 GiB | 1.48 GiB | ≤ 2 GiB |
| OCR `english.pdf` (PP-OCRv6 Small) | 0.79 s, exact | 0.30 s, exact | 0.32 s, exact | |
| OCR `russian.pdf` (Cyrillic) | 0.38 s, exact | 0.31 s, exact | 0.31 s, exact | |

Run 1 was the first in this boot to load the runtime libraries, so its cold
load is the true cold figure. Footprint after the scan, with the engine
dropped, is 115 MiB: the peak is transient, while the sessions are built and
first run.

## Stand-in: iOS Simulator

The harness in an iPhone 17 Pro Simulator (iOS 26.1, `iPhone18,1`) on the
same M4, core at `bce0bcc` built by `just ios-core` (`mobile` profile,
Microsoft's static ONNX Runtime 1.27.0 for iOS, PDFium 7988 as
`pdfium.framework`), run by `scripts/ios-spike-device.sh --simulator`. The
Simulator runs arm64 code natively but has neither the phone's CPU, its RAM
nor its thermals, and the Mac was overloaded during both runs (load average
420–490), so these times say nothing about a phone. Raw reports:
[run 1](simulator-iphone17pro-run1.json),
[run 2](simulator-iphone17pro-run2.json).

| Measurement | Run 1 | Run 2 | Budget |
| --- | ---: | ---: | --- |
| Cold load | 1.47 s | 1.29 s | ≤ 10 s |
| – ONNX Runtime into ort | 0.002 s | 0.002 s | |
| – GLiNER2 sessions | 1.47 s | 1.29 s | |
| Seconds per 512-token window (median) | 0.582 s | 0.539 s | |
| 20-page scan | 40.4 s | 39.9 s | ≤ 60 s |
| – windows / mean tokens / detections | 131 / 269 / 380 | 131 / 269 / 380 | |
| – inference | 38.2 s | 37.7 s | |
| – digest check, engine load | 0.37 s, 1.37 s | 0.34 s, 1.46 s | |
| Detector peak memory | 1.66 GiB | 1.66 GiB | ≤ 2 GiB |
| OCR `english.pdf` (PP-OCRv6 Small) | 0.75 s, exact | 0.62 s, exact | |
| OCR `russian.pdf` (Cyrillic) | 0.78 s, exact | 0.43 s, exact | |
| App executable (Release, stripped non-global) | 53 MB | | |

What the Simulator does establish:

- **The iOS runtime loading works.** ort found `OrtGetApiBase` in the app's
  own executable (`linked_runtimes` reported `<app>/MdocSpike`), and PDFium
  loaded from `<app>/Frameworks/pdfium.framework`, with the build settings
  in `tools/ios-spike/MdocSpike/Spike.xcconfig`. Loading ONNX Runtime costs
  2 ms, as it is already mapped.
- **The core gives the same result as desktop**: 131 windows and 380
  detections on the 20-page document, as on the Mac, and both OCR fixtures
  recognized exactly. AnyDoc converts the DOCX in-process.
- **Peak memory is higher than the Mac run** (1.66 against 1.41–1.48 GiB)
  with a SwiftUI app around it (106 MiB at start); on both phones the
  device peak was 1.96–1.97 GiB, within what the app may use (2.99 GiB on
  the iPhone 12 Pro, 3.29 GiB on the iPhone 17 Pro).
- `os_proc_available_memory` reads 0 in the Simulator, so the headroom
  figure exists only on a device.

## Findings for the hub

- **PDFium is an embedded framework, not static** (hub-approved option A).
  Neither approved source ships a static iOS PDFium: bblanchon's iOS builds,
  including `chromium/7988` (desktop's version), are dylibs, and firecrawl has
  no iOS assets. ADR 0085's invariant now reads "ship inside the app bundle
  (statically linked, or as an embedded, signed framework)". ONNX Runtime
  1.27.0's iOS build (Microsoft's pod archive) is static and is linked into
  `MdocCore.xcframework`.
- **bblanchon's iOS PDFium needs iOS 26.** Its `minos` is 26.0, so it cannot
  load on the iOS 17–25 that ADR 0085 supports. Before the App Store either
  PDFium is built for iOS 17 (from source, or another source the hub
  approves), or the minimum iOS rises to 26. The spike and the test phones
  run iOS 26, so the measurements are unaffected.
- **No dependency patches were needed, at the cost of three app build
  settings.** gliner2-rs 0.9.6 hard-enables ort's `load-dynamic` (and every
  execution-provider feature), and firecrawl-pdfium can only `dlopen` a path.
  So on iOS the core passes ort the path of its own image (found with
  `dladdr`), whose `OrtGetApiBase` the app must export and keep:
  `-Wl,-exported_symbol,_OrtGetApiBase`, `STRIP_STYLE = non-global` and
  `ENABLE_DEBUG_DYLIB = NO` (on macOS, default stripping removed the
  export; the Simulator run loaded it with these settings). `apps/ios` needs
  the same settings.
- **CoreML EP is available, unmeasured.** Microsoft's iOS ONNX Runtime
  includes the CoreML execution provider, and gliner2-rs can select it; ADR
  0002 keeps detection on CPU, and every number here is CPU. If a phone
  misses the scan budget, CoreML is the next thing to measure before INT8;
  the maintainer kept the 8 GB floor rather than measure either on the
  iPhone 12 Pro.
- **Peak memory is session build, not inference**: 1.4–1.5 GiB peak against
  115 MiB after the scan on the Mac, and 1.66 GiB in the Simulator app.
  On the phones memory was not what failed: the iPhone 12 Pro's 1.97 GiB
  peak left about 1 GiB of the 2.99 GiB the app may use.
- **The 2-minute scan deadline (ADR 0002) caps the measurement.** A phone
  slower than 120 s for 20 pages reports `scan_error` ("Scan exceeded two
  minutes") instead of a time; that is a budget fail either way. The
  iPhone 12 Pro hit it.
- **Open for later items**, not changed here: `ocr::SUPPORTED` stays false on
  iOS (the runtime download table is desktop's; `Setup` defines iOS's), and
  saved cases report no key store on iOS until `Library` adds the Keychain.

### Android (2026-10-31)

- GLiNER2 FP16 runs on every Android phone tried, within the memory, load
  and thermal budgets, but scans 3.6 to 9 times slower than the iPhone 17
  Pro, so no phone meets the 60 s budget for 20 pages.
- Not tried yet, cheapest first: more intra-op threads or pinning to the big
  cores; ONNX Runtime's XNNPACK execution provider, which the Android AAR
  ships and whose FP16 kernels use the ARMv8.2 FP16 units; the INT8 export
  (ADR 0085's fallback), which ships only if it matches FP16 on the
  qualification fixtures.
- Each run on Test Lab's free tier costs one of five physical-device runs a
  day; the harness downloads about 680 MB on the phone.
