# Require explicit offline OCR with qualified pinned models

Status: Accepted, 2026-10-01.

## Why

OCR is slow, can be wrong and needs downloaded models, and a privacy app
must not fetch or run them behind the user's back. So OCR runs only after an
explicit inline choice, even with a runtime installed, while native-text
conversion may run on opening. Only explicit setup downloads pinned,
checksum-verified components into app-local storage, and recognition runs
offline. A language is accepted only on actual recognition, never on
confidence or dictionary coverage: PP-OCRv6 Small reads Russian with high
confidence and wrong text, so the Cyrillic bundle pairs its detector with
PP-OCRv5 Cyrillic recognition through one manifest. The runtime is enabled
only on targets that passed scanned-PDF qualification (Apple Silicon macOS,
Windows x64); the rest keep native extraction. OCR failure never silently
falls back to native-only text.

## Rejected

- **OCR whenever a runtime is installed**: the user decides when to spend
  the time and accept recognized text.
- **PP-OCRv6 Small alone for Russian**: confident and wrong.
- **Accepting a target or language on a build or a confidence score**: only
  recognized fixture pages count.

## Revisit when

Another target or language passes the
[scanned-PDF qualification](../../tests/fixtures/ocr-qualification/README.md).
