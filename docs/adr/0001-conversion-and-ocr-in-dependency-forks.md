# Conversion and OCR belong in dependency forks

Status: Accepted, 2026-10-01.

Folds: 0007, 0008, 0009.

## Decision

- mdoc extracts documents through its forks of AnyDoc and pdf-inspector.
  Algorithm and output-quality changes belong in those forks, never in a
  second implementation in the app.
- The app owns scheduling, explicit OCR consent, warnings, the document
  lifecycle and the Markdown handoff. Converter and editor crates stay
  host-agnostic.
- mdoc pins the exact tested revision of each fork.
- **pdf-inspector:** upstream updates come in as complete merges into the
  fork, keeping its history and the local OCR extensions (pinned Cyrillic
  manifests, explicit runtime paths, offline recognition, explicit consent,
  recognition output before fusion).
- **AnyDoc:** only selected upstream changes are ported, each with
  attribution and only the mechanical changes it needs; a material deviation
  is reviewed first.
- **Zorite** (the editor, PDF viewer and BiDi crates' origin): only
  correctness and rendering fixes are ported, with attribution. Journal,
  property, template and import/export features stay out.

## Invariants

- A fork update is adopted only after the fork's checks with OCR enabled,
  mdoc's checks, and a before/after comparison on the controlled EN/RU
  fixtures and the [public corpus](../../tests/fixtures/ocr-upstream/README.md).
- Unexplained text loss, reading-order errors or invariant regressions block
  adoption; a build passing never validates an extraction change.
- macOS and Windows OCR smoke checks pass; an unavailable platform check is a
  stated blocker, never inferred acceptance.
- Ports preserve ordinary tables, revision semantics and the partial-PDF API,
  and every changed corpus output is explained.
- Editor ports preserve caret and selection, undo, source offsets, search
  geometry, diagnostic remapping and pseudonymization protections, and never
  normalize formatting on save or Copy Markdown.

References: [0007](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0007-review-upstream-converter-updates.md),
[0008](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0008-selected-anydoc-fidelity-fixes.md),
[0009](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0009-selected-zorite-upstream-fixes.md).
