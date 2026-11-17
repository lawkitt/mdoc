# Supervise DOCX preview in a bounded local worker

Status: Accepted, 2026-10-01. Amended 2026-11-18 (comments grill, #927).

Render DOCX locally to a temporary PDF using the same executable's
`--mdoc-docx-worker` mode. A supervised process bounds conversion time and can
be killed/reaped on cancellation; validation and rendering use one bounded
snapshot so archive inflation is inside the timeout. The read-only preview is
approximate and independent from Markdown extraction.

Reject encryption, macros, tracked changes, excessive archive entries/expanded
sizes, and oversized input. Current limits live in crates/mdoc-import/src/docx_preview.rs. Keep the
previous loaded preview on failure; discard stale results. Release the PDF view
before disposing its temporary file and cancel queued/running work on tab close.
[Synthetic fixtures](../../tests/fixtures/docx-preview/README.md)
provide reproducible worker/comment coverage without user documents.

**Renderer.** docxide-pdf, chosen over LibreOffice, platform renderers and
other Rust crates on Word fidelity (its published comparison against Word's
own export), speed and portability: pure Rust, so phones can run it in
process. It is a single-maintainer, fast-moving crate: exact pin, bumped at
each desktop milestone's start against the preview corpus and mdoc's
Word-reference fidelity check; forked only if a patch is unavoidable.

**Fonts.** Metric-compatible fallbacks (Carlito, Caladea, Liberation) ship
with the app and reach docxide as its `font_dirs` option; an installed
Calibri or Cambria still wins, because docxide looks up the requested
family first.

**Comments and hidden text** never enter the Markdown. The render copy marks
each commented run as a `mdoc-comment:<k>` link, whose rectangles anchor
mdoc's own comment rail, and lifts hidden text with a dotted underline when
shown; docxide's built-in comment pane stays off. Rules:
[originals](../specs/originals.md).
