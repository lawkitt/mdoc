# Mobile app grill, 2026-10-31

Interview record for an mdoc mobile app: Kotlin Multiplatform or native
(Swift, Kotlin), architecture, on-device recognition and detection, mobile UI.
Decisions land in ADRs once confirmed; this file keeps the research and the
open design tree.

## Research facts

### What mdoc runs today

| Part | Artifact | Size on disk |
| --- | --- | --- |
| OCR detector | PP-OCRv6 Small det (ONNX) | 9.9 MB |
| OCR recognizer, English | PP-OCRv6 Small rec | 21 MB |
| OCR recognizer, Russian | PP-OCRv5 Cyrillic mobile rec | 8 MB |
| PII detector, default | GLiNER2 PII FP16: encoder 556 MB, heads ~60 MB, tokenizer 16 MB | ~630 MB |
| PII detector, alt | GLiNER2 PII FP32 | ~1.25 GB |
| Runtime | ONNX Runtime 1.27 via `ort` 2.0.0-rc.13, `load-dynamic`, CPU only | 38 MB (macOS dylib) |
| PDF | PDFium (pdf-inspector fork) | 7 MB |

Logic is already GUI-free Rust (ADR 0069): `mdoc-pii`, `mdoc-history`,
`mdoc-detect`, `mdoc-import`, `mdoc-store`, `mdoc-spell`. The UI is GPUI,
which has no iOS or Android target.

### Feasibility on current phones

- **OCR: yes, easily.** PP-OCR's det/rec models are the "mobile" class
  (~30–40 MB) and run in well under a second per page on a recent phone CPU.
  Apple Vision reads Russian from iOS 16. Google ML Kit Text Recognition v2
  has no Cyrillic model (Latin, Chinese, Devanagari, Japanese, Korean only),
  so Russian on Android needs PP-OCR either way.
- **PII detection: yes on flagships, with caveats to measure.** GLiNER2 FP16
  is ~630 MB to download and likely 1–1.5 GB peak memory while scanning.
  iPhone 15 Pro and later have 8 GB (17 Pro: 12 GB); Android flagships have
  12–16 GB. iOS caps one app at a fraction of RAM; the
  `com.apple.developer.kernel.increased-memory-limit` entitlement raises it.
  Unknowns: ORT CPU fp16 kernels on ARM (casts may double memory), scan time
  per 512-token window, heat on long documents, cold load time. An INT8
  quantized export would roughly halve size but must pass the same
  qualification fixtures.
- **Accelerators.** ORT ships iOS (CoreML EP) and Android (XNNPACK; NNAPI is
  deprecated from Android 15; QNN for Snapdragon) builds. Transformer
  encoders often fall back to CPU partly under CoreML; CPU-only, as on
  desktop, is the safe baseline.

### Platform constraints that change the design

- **No downloaded code on iOS.** App Store guideline 2.5.2 forbids loading
  executable code fetched after install. Desktop downloads ONNX Runtime and
  PDFium at setup; on iOS both must be linked into the app (`ort` static
  linking, PDFium xcframework). Models are data and may still be downloaded.
- **No child processes on iOS.** The supervised DOCX preview worker
  (ADR 0005) cannot run as a process; DOCX conversion must run in-process
  or be left out.
- **Background limits.** iOS suspends apps shortly after they leave the
  foreground; `BGProcessingTask` runs when the OS chooses, not on demand. A
  two-minute scan runs in the foreground (or a short background extension).
  Android needs a foreground service with a notification.
- **Secrets.** Case keys move from macOS Keychain to iOS Keychain (Secure
  Enclave–backed) and Android Keystore.
- **Hand-off.** The desktop's copy/paste journey maps to the share sheet: a
  share extension to receive files, scans and AI replies; sharing out to the
  ChatGPT or Claude app or the clipboard.

### Stack options

- **Compose Multiplatform** for iOS is stable since 1.8.0 (May 2025); adds
  ~9 MB over SwiftUI. KMP shares Kotlin logic; mdoc's logic is Rust, so KMP
  either rewrites it or calls Rust through Gobley (third-party UniFFI KMP
  bindings, small maintainer base).
- **UniFFI** generates Swift and Kotlin bindings from Rust; Mozilla ships
  Firefox iOS and Android this way. Rust targets `aarch64-apple-ios` and
  `aarch64-linux-android` are tier 2.
- **Fully native** means writing pseudonymization, mapping, cases and
  restoration twice more, diverging from desktop behavior and the encrypted
  case format.

## Design tree

Round 1 (settled): product scope, platforms, code sharing, spike first.

Round 2 (settled): spike budgets and device floor, minimum OS, OCR engine,
intake, AI hand-off and restore, cases, review UI, repo and CI, licence.

Round 3 (settled): test devices (iOS first), floor rule, model delivery,
distribution, versions, language, accessibility and acceptance, spike output.

Round 4 (settled): Android while iOS goes first, when Android starts, iPad
and Mac, share extension and file intake, beta, storage and backups.

Round 5 (settled): state across the bridge, text display, original page,
long jobs, parity tests, roadmap placement.

Frontier: empty, pending the maintainer's confirmation.

## Decisions

Round 1, recorded in [ADR 0085](../adr/0085-mobile-apps.md):

1. Scope is the review-first flow: scan or import, recognize, review,
   share to AI, restore. No full editor; not a sync companion.
2. iOS and Android together.
3. Shared Rust core through UniFFI; SwiftUI and Jetpack Compose UI. No KMP.
4. A feasibility spike on real devices comes before any UI work.

Round 2, recorded in ADR 0085 and [ADR 0086](../adr/0086-app-store-licence-permission.md):

5. Detector budgets: peak ≤ 2 GB, cold load ≤ 10 s, 20 pages ≤ 60 s, no
   serious throttling; else INT8 if it matches FP16 on qualification; below
   the floor, OCR + structured rules + manual marking. Test phones: iPhone
   17 Pro (12 GB) and iPhone 12 Pro (6 GB); no Android phone yet.
6. iOS 17+, Android 10 (API 29)+. Changed to iOS 26+ on 2026-10-31: the
   approved iOS PDFium build needs iOS 26 (maintainer's decision).
7. PP-OCR everywhere; no ML Kit text recognition.
8. Intake: camera scan, photos, PDF, DOCX (in-process AnyDoc, no DOCX
   preview), pasted text; picker and share sheet.
9. Share or Copy out with the placeholder note; Restore with mdoc share
   action or Paste back.
10. A case per document by default; desktop format; Keychain/Keystore; no
    sync.
11. Read-only review with tap sheets, Identities sheet, select-to-replace.
12. Monorepo: `crates/mdoc-ffi`, `apps/ios`, `apps/android`; local CI jobs.
13. GPLv3 section 7 app-store permission; DCO for outside contributions.

Round 3, recorded in ADR 0085:

14. iOS first. Android later, tested on a friend's recent Android phone; no
    Firebase Test Lab for now. Supersedes decision 2's "together".
15. Floor: 6 GB if the iPhone 12 Pro meets every budget, else 8 GB; INT8
    fills the gap if it passes; setup checks RAM.
16. Own verified background download, Wi-Fi by default.
17. Free, no accounts or analytics; App Store; later Google Play and APK,
    RuStore on demand; local crash reports.
18. Separate `ios-v*` / `android-v*` versions from `main`.
19. English, all strings in catalogs.
20. VoiceOver, Dynamic Type, dark mode in acceptance; agent runs Simulator
    rows, maintainer runs device rows.
21. Spike keeps `mdoc-ffi`, throws away the Swift harness, ends in an
    evidence report the maintainer reads before UI work.

Round 4, recorded in ADR 0085:

22. Local CI `cargo check`s `mdoc-ffi` for Android until Android starts.
23. Android starts after iOS 0.1 ships and gets first feedback.
24. iPhone and iPad from v1; no Mac.
25. Files via document types; Restore with mdoc share extension restores
    in place with the last-shared case and a picker.
26. TestFlight beta, "Data Not Collected", review notes on model data.
27. No backups of documents, cases or models; Data Protection Complete;
    this-device-only case key.

Round 5, recorded in ADR 0085 and ROADMAP.md:

28. Rust owns state; UniFFI exposes `Library`, `DocumentSession`, `Setup`;
    platforms hold view state; Swift downloads, Rust verifies.
29. Markdown source styled with dimmed markers; span recognition moves to a
    GUI-free crate; read-only TextKit 2 view with Replace….
30. PDFKit and an image view for originals; Text / Original on iPhone,
    side by side on iPad.
31. One model job; `BGContinuedProcessingTask` on iOS 26+, otherwise grace
    period, cancel and offer to resume.
32. Core tests, Swift parity tests against desktop goldens, one XCUITest.
33. "iOS 0.1" roadmap section after 0.3.0; core build changes after rc.2.

### Facts found in round 5 research

- The desktop editor shows Markdown source with markers dimmed, so
  candidate offsets are source offsets. Its recognizer,
  `mdoc-editor/src/markdown_syntax.rs` (3.4k lines), returns GPUI colours
  and fonts in 23 places; the span recognition itself needs no GPUI.
- `BGContinuedProcessingTask` (iOS 26) lets a user-started task keep
  running in the background with system progress UI; iOS 17–25 has only
  the ~30 s `beginBackgroundTask` grace.
- A background `URLSession` is Swift-only; Rust can still verify and
  install the downloaded file.

### Facts found in round 2 research

- mdoc is GPL-3.0-or-later. The FSF holds that App Store terms conflict
  with the GPL (VLC was pulled in 2011). All non-bot commits are the
  maintainer's, and the core crates pull in no third-party GPL code, so the
  copyright holder can add an App Store exception or dual-license.
