# Require explicit offline OCR with qualified pinned models

Status: accepted, consolidated 2026-10-01. [ADR 0006](0006-local-model-settings.md)
adds model selection and fixes explicit OCR to Force on every selected page;
native opening and explicit recognition consent remain separate.

OCR recognition requires an explicit inline choice, including when a runtime is
already installed. Only explicit setup downloads pinned, checksum-verified
components into app-local storage; recognition is offline and document contents
stay local. This preserves user control while allowing automatic native-text
conversion on opening.

The original PP-OCRv6 Small recognizer ran successfully but failed Russian text
despite high confidence. The qualified fork therefore uses the PP-OCRv6 Small
detector with PP-OCRv5 Cyrillic recognition and its matching dictionary through
one manifest. Language acceptance requires actual recognition, not confidence or
dictionary coverage alone. Manifest identity/digests participate in the fork's
cache key; explicit runtime paths avoid process-wide environment mutation.

Runtime support is enabled only after actual scanned-PDF qualification: Apple
Silicon macOS and Windows x64 currently qualify. Other targets retain native
extraction. Windows ONNX Runtime additionally requires the x64 Visual C++
Redistributable, despite mdoc.exe's static CRT; loader failures retain verified
downloads and provide installation/retry instructions.

Partial output keeps page-specific warnings outside Markdown; OCR failure never
silently falls back to native-only extraction. No usable output preserves the
current document. Setup can finish globally, but document continuation remains
generation-scoped. Percentage progress and immediate cancellation are not
promised by synchronous converter calls. Image-only DOCX OCR is not implemented.

The exact pins/digests belong in Cargo.toml, Cargo.lock, src/ocr.rs, and the fork
manifest. [Fixture qualification](../../tests/fixtures/ocr-qualification/README.md)
retains the useful historical evidence and reproduction instructions.
