# Select three narrow AnyDoc fidelity fixes

Status: accepted, 2026-10-04. The user confirmed the design and authorized
implementation with "design confirmed, implement".

## Decision

For the lightweight PDF/Office-to-Markdown utility, select only upstream AnyDoc
PRs [#17](https://github.com/firecrawl/anydoc/pull/17),
[#177](https://github.com/firecrawl/anydoc/pull/177), and
[#174](https://github.com/firecrawl/anydoc/pull/174).

- Preserve readable tables in Word forms by unwrapping single-cell containers
  around nested tables; do not broadly rewrite genuine nested grids.
- Preserve supported DOCX checkbox states, styling and text order.
- Produce final text from legacy DOC revisions: omit deleted revision content,
  retain insertions and ordinary strikethrough, and preserve structural delimiters.

Every other upstream PR and issue is outside this update. Preserve the existing
fork behavior and document lifecycle contracts.

## Rationale

These changes recover structure or meaning in already supported Office formats
without expanding the utility into new formats, asset management or OCR providers.
mdoc handles PDFs through its separate pdf-inspector integration.

## Delivery and validation

Port the selected behavior and tests with only necessary mechanical adjustments.
Review any material deviation before adopting it. Deliver a fork review PR and an
mdoc review PR with the exact tested fork revision and lockfile.

Require fork formatting, lint and tests, focused mdoc import regression coverage,
and before/after fixtures demonstrating each improvement. Existing corpus changes
must be explained; preserve ordinary tables, unknown-symbol behavior, revision
semantics and the existing partial-PDF API. Automated converter tests establish
conversion behavior, not native or cross-platform UI acceptance.

See [the decision record and full inventory](../design/anydoc-upstream-triage.md).

## Implementation evidence

All selected upstream commits were ported with attribution, without conflicts or
functional changes. The new #17 snapshot was adapted to the base's existing
header inference. mdoc pins tested fork
`97d21d1f46086d84478da072ba1fdfa210461c80`.

The [verification record](../../tests/fixtures/import/anydoc-fidelity-verification.md)
reports 318 passing fork tests, 461 passing mdoc tests, lint/format/source-policy
checks and before/after output evidence. All 75 pre-existing fork fixture outputs
and all existing mdoc expectations were unchanged. Three added synthetic import
fixtures exercise the selected improvements and source preservation.

Review delivery: [AnyDoc fork PR](https://github.com/lawkitt/anydoc/pull/1) and an
mdoc review branch/PR. No main branch was merged during this implementation.
