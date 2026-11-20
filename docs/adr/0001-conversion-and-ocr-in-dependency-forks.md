# Conversion and OCR belong in dependency forks

Status: Accepted, 2026-10-01.

## Decision

- mdoc extracts documents through its forks of AnyDoc and pdf-inspector,
  separate repositories pinned by exact tested revision; no subtree,
  merged crate or crates.io release. Algorithm and output-quality changes
  belong in the forks, never in a second implementation in the app.
- The app owns scheduling, explicit OCR consent, warnings, the document
  lifecycle and the Markdown handoff. Converter and editor crates stay
  host-agnostic.
- Every PDF goes to pdf-inspector; mdoc builds AnyDoc without its `pdf`
  feature.
- **pdf-inspector:** upstream updates come in as complete merges into the
  fork, keeping its history and the local OCR extensions (pinned Cyrillic
  manifests, explicit runtime paths, offline recognition, explicit consent,
  recognition output before fusion).
- **AnyDoc:** a squashed snapshot of upstream; upstream changes come in as
  selected cherry-picks with only the mechanical changes they need.
- Either fork may port an unmerged community PR, with its author's
  attribution and the same evidence. Fork changes are not offered upstream.
- Upstream is checked at the start of each desktop milestone and on demand
  for user-facing fixes.
- Hidden content (comments, hidden-formatted or invisible text, form field
  names) never enters the Markdown; the forks report what they left out
  (comments and hidden passages as structured data: author, date, text,
  anchor, replies, resolved) and mdoc counts or shows it.
- **Zorite** (the editor, PDF viewer and BiDi crates' origin): only
  correctness and rendering fixes are ported, with attribution. Journal,
  property, template and import/export features stay out.

## Invariants

- A fork update is adopted only after the fork's checks with OCR enabled,
  mdoc's checks, and a before/after comparison on the controlled EN/RU
  fixtures and the [public corpus](../../tests/fixtures/ocr-upstream/README.md).
- A structural extraction change needs evidence and its own review.
- Unexplained text loss, reading-order errors or invariant regressions block
  adoption; a build passing never validates an extraction change.
- macOS and Windows OCR smoke checks pass; an unavailable platform check is a
  stated blocker, never inferred acceptance.
- Ports preserve ordinary tables, revision semantics, unknown-symbol
  behaviour and the partial-PDF API,
  and every changed corpus output is explained.
- Editor ports preserve caret and selection, undo, source offsets, search
  geometry, diagnostic remapping and pseudonymization protections, and never
  normalize formatting on save or Copy Markdown.

References: [0007](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0007-review-upstream-converter-updates.md),
[0008](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0008-selected-anydoc-fidelity-fixes.md),
[0009](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0009-selected-zorite-upstream-fixes.md).
