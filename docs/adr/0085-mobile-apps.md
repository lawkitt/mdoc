# Mobile apps

Status: Accepted, 2026-10-31.

## Problem

Lawyers use AI apps on phones; mdoc ran only on desktop, and GPUI has no
mobile target.

## Decision

- **Scope: the review-first flow:** open or scan, review the proposed
  replacements, share to an AI app, restore the reply. No sync with desktop.
- **iOS first, then Android**; iPhone and iPad are one app.
- **Shared Rust core, native UI.** The apps call the GUI-free crates through
  UniFFI bindings in `crates/mdoc-ffi`. SwiftUI on iOS, Jetpack Compose on
  Android. No Kotlin or Compose Multiplatform, and no core logic in Swift or
  Kotlin.
- **State across the bridge.** Rust owns document, review, case and restore
  state behind three coarse objects: `Library`, `DocumentSession` and
  `Setup`. The platforms hold view state only, run background downloads and
  report progress and cancellation; Rust verifies and installs.
- **Minimum iOS 26**, because the approved PDFium build requires it.
  Android's is [ADR 0089](0089-android-app.md)'s.
- **Runtimes in the bundle.** ONNX Runtime and PDFium ship inside the app,
  statically linked or as an embedded signed framework.
- **Detector budgets** per device class: peak memory ≤ 2 GiB, cold load
  ≤ 10 s, a 20-page scan ≤ 60 s, no serious throttling. The floor is set by
  physical RAM and measured
  ([report](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/evidence/mobile/2026-10-31/README.md));
  setup offers only the models a device can run. An INT8 export ships only
  if it matches FP16 on the qualification fixtures.
- **Models** come from desktop's pinned URLs and SHA-256 checks, on Wi-Fi
  unless the user allows cellular; no Background Assets or Play Asset
  Delivery.
- **OCR is PP-OCR** through the core; ML Kit text recognition has no
  Cyrillic.
- **Storage.** Documents, Markdown, cases and models are excluded from
  backups; documents and cases use Data Protection Complete; the case key is
  this-device-only, in the Keychain or Keystore. A new phone starts empty.
- **Cases** use the desktop's encrypted format and stay on the device.
- **Restore with mdoc** is a share extension that needs no model and never
  mixes cases.
- **Distribution:** free, no accounts, no analytics.

## Invariants

- The same document and model give the same candidates, aliases and restored
  text on desktop and mobile.
- No network calls except verified model downloads, and no code downloaded
  after install.
- Shared text always passes through desktop's Copy for AI export path.

## Consequences

- Each screen is written twice. Mobile-only core code sits behind
  `cfg(target_os = …)`.

References: [interview](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/design/mobile-grill-2026-10-31.md); rules in [docs/specs/mobile.md](../specs/mobile.md).
