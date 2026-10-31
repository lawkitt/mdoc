# Mobile apps

Status: Accepted, 2026-10-31. Amended 2026-10-31: core-crate changes merge
when green, UI work runs beside the spike, native runtimes ship inside the
app bundle, statically linked or as an embedded framework, and the minimum is
iOS 26.

## Problem

mdoc runs only on macOS and Windows. Lawyers receive documents on their
phones and use AI apps there, but a phone has no way to remove client names
before the text reaches an AI. GPUI, mdoc's UI framework, has no iOS or
Android target, so a mobile app needs a new UI layer. The logic it needs
already lives in GUI-free Rust crates (ADR 0069).

## Decision

- **Scope: the review-first flow.** The mobile app opens or scans a
  document, recognizes it, proposes replacements for review, shares the
  pseudonymized text to an AI app and restores the AI's reply. It has no
  full Markdown editor. It is not a companion to the desktop app, so it
  needs no sync to be useful.
- **Platforms: iOS first, then Android.** Both apps use the same core;
  neither is a port of the other. Android starts after iOS 0.1 is on the
  App Store and its first feedback has shaped the review screen; it is
  tested on a borrowed recent Android phone. Until then the Mac's local
  CI runs `just mobile`, a `cargo check` of `mdoc-ffi` for iOS and Android,
  so desktop-only code does not creep into the core.
- **iPhone and iPad** in one app from v1. "Designed for iPad" on Apple
  silicon Macs is off; the desktop app covers the Mac.
- **Shared Rust core, native UI.** The mobile apps call the existing
  GUI-free crates (`mdoc-pii`, `mdoc-history`, `mdoc-detect`,
  `mdoc-import`, `mdoc-store`) through UniFFI bindings. The iOS UI is
  SwiftUI; the Android UI is Jetpack Compose. No Kotlin Multiplatform, no
  Compose Multiplatform, and no reimplementation of detection, replacement,
  case or restoration logic in Swift or Kotlin.
- **Minimum OS: iOS 26 and Android 10 (API 29).** iOS 26 because the
  approved iOS PDFium build (bblanchon chromium/7988) requires it; every
  iPhone that runs iOS 17 also runs iOS 26.
- **State across the bridge.** Rust owns document, review, case and restore
  state as ADR 0069 owners. UniFFI exposes three coarse objects:
  `Library` (cases, stored documents, storage paths), `DocumentSession`
  (import and OCR, scan, review transitions, Copy for AI and Share text)
  and `Setup` (models that fit the device, verifying and installing
  downloads). Swift and Kotlin hold view state only, built from what Rust
  returns. Rust calls the platform only for progress and cancellation. The
  platform runs the background download; Rust verifies and installs it.
- **Text display.** The document shows its Markdown source styled as the
  desktop editor does, with markers dimmed, so candidate offsets are the
  core's source offsets. The GPUI-free span recognition of
  `mdoc-editor`'s `markdown_syntax` moves to a GUI-free crate that both
  desktop and mobile use; each platform maps spans to its own colours and
  fonts. iOS draws it in a read-only, selectable TextKit 2 `UITextView`
  whose edit menu adds Replace….
- **Original page.** PDFKit shows PDFs and a zoomable image view shows scans
  and photos; PDFium stays in the core for extraction and OCR. iPhone
  switches Text / Original; iPad shows them side by side.
- **Long jobs.** One model job at a time (ADR 0006), off the main thread,
  with progress and cooperative cancellation. Leaving the app continues a
  user-started job through `BGContinuedProcessingTask`; when the system
  refuses one, the job gets the short background grace, then cancels, and
  the app offers to resume on return (OCR keeps finished pages, the scan
  reruns). Nothing starts in the background on its own.
- **Feasibility spike first.** Before any UI work, `mdoc-detect` and
  `mdoc-import` are built for iOS and Android with ONNX Runtime and PDFium
  inside the app bundle, and measured on an iPhone 17 Pro (12 GB, top) and an
  iPhone 12 Pro (6 GB, floor): cold model load, peak memory, seconds per
  512-token window, thermal state over a 20-page document, and OCR output
  on the existing fixtures. The spike keeps the real `crates/mdoc-ffi` and
  the core's platform features; its Swift harness app, which runs the core
  on fixtures and writes timings as JSON, is thrown away. The result is a
  dated report under `docs/evidence/` with a pass or fail per device class.
  UI work runs beside the spike; Simulator and Mac numbers stand in until
  the device rows are measured, and the detector budgets still decide what
  ships. Android repeats the spike
  when its work starts.
- **Detector budgets.** GLiNER2 FP16 ships on a device class when it meets
  all of: peak memory ≤ 2 GB, cold load ≤ 10 s, a 20-page document scanned
  in ≤ 60 s, and no serious thermal throttling. If FP16 fails, an INT8
  export is tried and ships only if it matches FP16 on the pseudonymization
  qualification fixtures. Devices below the floor get OCR, the structured
  rules (emails, phones, IBAN, INN/SNILS and the rest of `mdoc-detect`'s
  rules) and manual marking, labelled as such. The floor is set by physical
  RAM, not OS version: 6 GB if the iPhone 12 Pro meets every budget,
  otherwise 8 GB; INT8 fills the gap if it passes where FP16 fails. Setup
  checks RAM and offers only the models the device can run.
- **Model delivery.** Models come from the same pinned URLs and SHA-256
  checks as desktop, through a background download session, on Wi-Fi by
  default; cellular needs the user's yes. A first-use setup card names the
  size (ADR 0026). No Apple Background Assets or Play Asset Delivery.
- **OCR: PP-OCR everywhere** through the Rust core, the same bundles as
  desktop. OS engines follow the desktop's Later item (adopted where they
  beat PP-OCR on the fixtures). ML Kit text recognition is excluded: it has
  no Cyrillic model.
- **Intake.** Camera scans (VisionKit document camera on iOS, ML Kit
  Document Scanner on Android; capture only, recognition stays PP-OCR),
  photos, PDF, DOCX and pasted text, from the in-app picker and the share
  sheet. DOCX converts in-process through AnyDoc with no visual DOCX
  preview; PDFs and scans show the original page beside the text.
- **AI hand-off and restore.** Out: Share (text or a `.md` file) or Copy,
  both carrying Copy for AI's placeholder note. Back: the **Restore with
  mdoc** share action or Paste in the app; the restored reply opens in mdoc
  and can be copied or shared.
- **iOS entry points.** Files arrive through registered document types
  ("Open in mdoc" in the share sheet's app row), with no extension. Restore
  with mdoc is a share extension for text that restores in place: it needs
  no model, uses the case of the most recently shared document with a
  picker, never mixes cases (ADR 0084's rule), and offers Copy, Share and
  Save to document, which stores the document's reply (ADR 0078). The app
  and the extension share cases through an App Group container and the
  case key through a Keychain access group.
- **On-device storage.** Documents, Markdown, cases and models are excluded
  from iCloud and device backups. Documents, Markdown and cases use Data
  Protection Complete; the case key is this-device-only. A new phone starts
  empty, and Settings says so.
- **Cases.** Each prepared document starts a new case unless the user adds
  it to an existing one, so restore always has a mapping. Cases use the
  desktop's encrypted format; the case key lives in the iOS Keychain or the
  Android Keystore. No sync and no transfer between devices.
- **Review screen.** Read-only rendered text with candidates underlined.
  Tapping one opens a sheet: alias, Keep or Replace, all occurrences or
  just this one. An Identities sheet lists identities by category. Missing
  text is added by selecting it and choosing Replace…. No text editing. On
  tablets the identities list sits beside the text.
- **Parity tests.** Rust core tests run as today. A Swift XCTest suite
  calls the core through `mdoc-ffi` on the shared fixtures and checks
  candidates, aliases and restored text against the desktop's goldens. A
  short XCUITest walks open → review → share → restore in the Simulator.
  The local CI iOS job runs both.
- **Repository and CI.** In this repository: `crates/mdoc-ffi` (UniFFI
  bindings), `apps/ios`, `apps/android`. Local CI (ADR 0082) gains an iOS
  job on the Mac runner (build and XCTest in the Simulator), run when a PR
  touches the core crates, `mdoc-ffi` or `apps/`; an Android job on ubuntu
  (build and emulator tests) joins when Android work starts.
- **Distribution.** Free, no accounts, no analytics. iOS goes through a
  TestFlight beta (the maintainer, then a small external group of lawyers)
  to the App Store, with the privacy label "Data Not Collected" and review
  notes explaining that the optional download is model data, not code. Android, when it comes, ships on Google Play and as an APK on
  GitHub Releases; RuStore follows if Russian-speaking users ask. Crash
  reports stay on the device and show at the next launch (ADR 0047).
- **Versions.** Each app is versioned on its own (`ios-v0.1.0`,
  `android-v0.1.0`), built from `main`, and records the core commit it was
  built from.
- **Language.** English UI, every string in a String Catalog (iOS) or
  `strings.xml` (Android) from the start; Russian follows the desktop's
  localization item.
- **Accessibility and acceptance.** VoiceOver (and TalkBack on Android),
  Dynamic Type and dark mode are acceptance rows from v1: underlined
  candidates, the alias sheet and the Identities list work by screen reader.
  `docs/platform-acceptance.md` gains a section per app; Simulator rows are
  run by an agent, physical-device rows (camera scan, sharing to ChatGPT or
  Claude, heat) by the maintainer.
- **Licence.** Store distribution is covered by
  [ADR 0086](0086-app-store-licence-permission.md).

## Invariants

- Behaviour defined by the Rust core is the same on every platform: the
  same document and model give the same candidates, aliases and restored
  text on desktop and mobile.
- Nothing is uploaded. The mobile apps make no network calls except
  verified model downloads, as on desktop (ADR 0002), and keep documents
  and cases out of backups.
- Shared text always passes through the same export path as desktop Copy
  for AI: no share or copy action sends originals unless the user chose
  Keep for them.
- No code is downloaded after install on either platform: ONNX Runtime and
  PDFium ship inside the app bundle (statically linked, or as an embedded,
  signed framework); only model files are downloaded.

## Consequences

- The work is the roadmap's iOS 0.1 section. Changes to the core crates
  merge through the normal gate and leave desktop unchanged: new Cargo
  features are off by default and mobile-only code sits behind
  `cfg(target_os = "ios")` or `cfg(target_os = "android")`.
- Each screen is written twice (SwiftUI and Compose); the review-first
  scope keeps the screen count small.
- A case made on the phone stays on the phone; restoring a reply on the
  desktop needs the case made there.
- The core crates must build for `aarch64-apple-ios` and
  `aarch64-linux-android`; desktop-only pieces (the DOCX preview worker,
  `load-dynamic` runtime loading, macOS Keychain access) move behind
  platform features.

References: [interview](../design/mobile-grill-2026-10-31.md).
