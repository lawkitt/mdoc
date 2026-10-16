# Select three narrow AnyDoc fidelity fixes

Status: Accepted, 2026-10-04.

## Decision

Port only upstream AnyDoc PRs
[#17](https://github.com/firecrawl/anydoc/pull/17),
[#177](https://github.com/firecrawl/anydoc/pull/177) and
[#174](https://github.com/firecrawl/anydoc/pull/174):

- Unwrap single-cell containers around nested tables in Word forms; genuine
  nested grids are not rewritten.
- Preserve supported DOCX checkbox states, styling and text order.
- Produce final text from legacy DOC revisions: omit deleted content, keep
  insertions and ordinary strikethrough, preserve structural delimiters.

Every other upstream PR and issue is out of scope. Ports keep attribution and
only necessary mechanical changes; any material deviation is reviewed first.
Ordinary tables, unknown-symbol behavior, revision semantics and the partial-PDF
API are preserved, and existing corpus changes must be explained.

## Rationale

These recover structure or meaning in formats mdoc already supports without
expanding into new formats, assets or OCR providers. PDFs go through
pdf-inspector (ADR 0007).

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0008-selected-anydoc-fidelity-fixes.md),
[inventory](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/anydoc-upstream-triage.md),
[verification](../../tests/fixtures/import/anydoc-fidelity-verification.md).
