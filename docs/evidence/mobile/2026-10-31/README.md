# Mobile feasibility spike, 2026-10-31

Measurements for [ADR 0085](../../../adr/0085-mobile-apps.md)'s feasibility
spike: GLiNER2 FP16 and PP-OCR through the shared Rust core
(`crates/mdoc-ffi`) on the iOS targets. The iPhone 17 Pro row is measured
(below); the iPhone 12 Pro row is Pending (maintainer). The Apple M4 Mac and
iOS Simulator sections are stand-ins that do not decide what ships.

## Budgets (ADR 0085)

GLiNER2 FP16 ships on a device class when it meets all of:

| Budget | Limit |
| --- | --- |
| Peak memory (`phys_footprint`) | ≤ 2 GiB |
| Cold model load | ≤ 10 s |
| A 20-page document scanned | ≤ 60 s |
| Thermal state | no serious throttling |

If FP16 fails, an INT8 export is tried; devices below the RAM floor (6 GB if
the iPhone 12 Pro passes every budget, otherwise 8 GB) get OCR, the
structured rules and manual marking.

## Device classes

| Device | RAM | Result | Report |
| --- | --- | --- | --- |
| iPhone 17 Pro (top) | 12 GB | **Borderline**: every budget passes except memory, which depends on the unit (2.12 GB = 1.97 GiB against "2 GB") | [iphone-17-pro.json](iphone-17-pro.json) |
| iPhone 12 Pro (floor) | 6 GB | **Pending (maintainer)** | — |

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

`iPhone18,1`, 12 GB, iOS 27.0.1, core `a41003f` (`mobile` profile, ONNX
Runtime 1.27.0 CPU, PDFium 7988), measured 2026-10-31 on the phone's own
screen, launched by hand after the certificate check, the phone on its USB-C
cable (charging) and at 100 %. Raw report: [iphone-17-pro.json](iphone-17-pro.json).

| Measurement | iPhone 17 Pro | Budget |
| --- | ---: | --- |
| Cold load (runtime + GLiNER2 sessions) | 0.91 s | ≤ 10 s |
| Seconds per 512-token window (median of 5) | 0.489 s | |
| 20-page scan | 43.2 s | ≤ 60 s |
| Detector peak memory (`phys_footprint`) | 2,116,783,624 bytes: 2.12 GB, 1.97 GiB | ADR 0085: ≤ 2 GB |
| Memory available to the app at start | 3.29 GiB | |
| Thermal state | nominal, then fair from 20 s | below serious |
| OCR `english.pdf` / `russian.pdf` | 0.46 s / 0.40 s, both exact | |

Load, scan time and heat pass with room to spare. Memory does not settle on
its own: ADR 0085 says 2 GB, the harness checks 2 GiB (its JSON says pass),
and the peak falls between the two, 6 % over 2 GB and 1.5 % under 2 GiB. The
maintainer decides which limit the ADR means; until then this row is
borderline, not a pass. Either way the app may use only about 3.3 GiB of the
phone's 12 GB without the increased-memory-limit entitlement. On the 6 GB iPhone 12 Pro the same 1.97 GiB peak is likely to
exceed what the app may use, which points the floor at 8 GB with INT8 for
smaller phones; the 12 Pro row decides.

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
  with a SwiftUI app around it (106 MiB at start); the iPhone 12 Pro row
  decides whether that fits its limit.
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
  misses the scan budget, CoreML is the next thing to measure before INT8.
- **Peak memory is session build, not inference**: 1.4–1.5 GiB peak against
  115 MiB after the scan on the Mac, and 1.66 GiB in the Simulator app. The phone rows decide; if the iPhone 12
  Pro's jetsam limit is close, the harness's `available_memory_at_start`
  shows the headroom, and the increased-memory-limit entitlement is an
  option.
- **The 2-minute scan deadline (ADR 0002) caps the measurement.** A phone
  slower than 120 s for 20 pages reports `scan_error` ("Scan exceeded two
  minutes") instead of a time; that is a budget fail either way.
- **Open for later items**, not changed here: `ocr::SUPPORTED` stays false on
  iOS (the runtime download table is desktop's; `Setup` defines iOS's), and
  saved cases report no key store on iOS until `Library` adds the Keychain.
