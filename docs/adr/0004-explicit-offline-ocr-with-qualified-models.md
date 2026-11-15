# Require explicit offline OCR with qualified pinned models

Status: Accepted, 2026-10-01.

## Decision

- OCR runs only after an explicit inline choice, even when a runtime is
  installed. Native-text conversion may run automatically on opening.
- Only explicit setup downloads pinned, checksum-verified components into
  app-local storage. Recognition is offline and document contents stay local.
- The English default is PP-OCRv6 Small. The Cyrillic bundle pairs the
  PP-OCRv6 Small detector with PP-OCRv5 Cyrillic recognition and its
  dictionary through one manifest, because PP-OCRv6 Small fails Russian text
  despite high confidence.
- Manifest identity and digests are part of the fork's cache key; explicit
  runtime paths avoid process-wide environment changes.
- Runtime support is enabled only on targets that passed scanned-PDF
  qualification: Apple Silicon macOS and Windows x64. Other targets keep
  native extraction.
- Windows ONNX Runtime needs the x64 Visual C++ Redistributable; loader
  failures keep verified downloads and show install and retry instructions.

## Invariants

- Language acceptance requires actual recognition, never confidence or
  dictionary coverage alone.
- OCR failure never silently falls back to native-only extraction, and no
  usable output keeps the current document.
- Page-specific warnings of partial output stay outside the Markdown.
- Setup finishes globally; continuing a document stays generation-scoped.
- Percentage progress and immediate cancellation are not promised by the
  synchronous converter calls. Image-only DOCX OCR is not implemented.

The exact pins and digests live in `Cargo.toml`, `Cargo.lock`,
`crates/mdoc-import/src/ocr.rs` and the fork manifest.
[Fixture qualification](../../tests/fixtures/ocr-qualification/README.md) keeps
the reproduction instructions.
