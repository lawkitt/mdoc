# Mobile apps

Status: Accepted, 2026-10-31.

## Why

Lawyers use AI apps on their phones, and GPUI has no mobile target. The
phones get the review-first flow (open or scan, review, share to an AI app,
restore the reply), free, with no accounts and no sync. iOS and Android
share the Rust core through UniFFI (`crates/mdoc-ffi`) under native SwiftUI
and Jetpack Compose, so every platform gives a document the same
candidates, aliases and restored text. The core decides every rule about
documents, Projects, review, aliases, file names and restore, and the apps
display its snapshots: when Swift computed review rules, two were wrong,
and a fake core let tests pass on behaviour the core lacked, so tests run
the real core. ONNX Runtime, PDFium and docxide ship inside the apps;
nothing downloads after install but verified models, and nothing goes into
backups, so a new phone starts empty. iOS 26 is the minimum for the
approved PDFium build, Android 12 for `dataExtractionRules`.

## Rejected

- **Kotlin or Compose Multiplatform, or rules in Swift or Kotlin**: a second
  implementation of the core.
- **Google's SDKs**: ML Kit has no Cyrillic text recognition and needs Play
  services, and Firebase and analytics send data; scans use the system
  camera.
- **Background Assets or Play Asset Delivery**: models come from desktop's
  pinned URLs and checksums.

## Revisit when

A review tap on the iPhone 12 Pro passes about 16 ms, so core calls leave
the main actor; or GPUI gains a mobile target.
