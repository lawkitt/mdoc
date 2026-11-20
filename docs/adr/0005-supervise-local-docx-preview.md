# Supervise DOCX preview in a bounded local worker

Status: Accepted, 2026-10-01.

## Why

A Word document needs a faithful read-only preview beside its Markdown, but
a crafted DOCX can hang or exhaust a renderer. Rendering runs in the same
executable's `--mdoc-docx-worker` mode, a supervised process that bounds the
time and is killed on cancel; validation and rendering share one bounded
snapshot, so archive inflation counts against the timeout. Encrypted, macro,
tracked-change and oversized documents are refused. The preview is
approximate and independent of extraction. The renderer is docxide-pdf: pure
Rust, so phones run it in process, and closest to Word's own export on its
published comparison. It is a fast-moving single-maintainer crate, so it is
pinned exactly and bumped at each desktop milestone against the preview
corpus. Metric-compatible fonts (Carlito, Caladea, Liberation) ship with the
app; an installed Calibri or Cambria still wins.

## Rejected

- **LibreOffice, platform renderers and other Rust crates**: behind docxide
  on Word fidelity, speed or portability.
- **Rendering inside the app process on desktop**: a hung conversion could
  be neither bounded nor killed.

## Revisit when

docxide stalls or needs a patch mdoc can't wait for: then fork it.
