# Supervise DOCX preview in a bounded local worker

Status: accepted current behavior, consolidated 2026-10-01.

Render DOCX locally to a temporary PDF using the same executable's
`--mdoc-docx-worker` mode. A supervised process bounds conversion time and can
be killed/reaped on cancellation; validation and rendering use one bounded
snapshot so archive inflation is inside the timeout. The read-only preview is
approximate and independent from Markdown extraction.

Reject encryption, macros, tracked changes, excessive archive entries/expanded
sizes, and oversized input. Current limits live in src/docx_preview.rs. Keep the
previous loaded preview on failure; discard stale results. Release the PDF view
before disposing its temporary file and cancel queued/running work on tab close.
Extract comments separately as read-only metadata, allowing empty comment text;
they never enter the Markdown. [Synthetic fixtures](../../tests/fixtures/docx-preview/README.md)
provide reproducible worker/comment coverage without user documents.
