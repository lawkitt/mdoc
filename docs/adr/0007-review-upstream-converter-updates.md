# Review upstream converter updates against preserved fork behavior

Status: accepted, 2026-10-04, including the public corpus and final
shared-understanding confirmation. Implementation authorized.

## Decision

For this pdf-inspector update, merge the complete upstream revision
`ef52f77850b29189048797a48b24862251f54fe7` (1.25.2) into the fork while preserving
its history and the two local OCR extensions. Deliver the fork update and mdoc's
exact tested dependency pin through review branches/PRs.

Preserve pinned Cyrillic model manifests, explicit runtime paths, offline
recognition, explicit app consent, and optional recognition output before fusion.
Existing document-lifecycle and source-preservation contracts remain invariants.

Require fork checks with OCR enabled, mdoc checks, and before/after output
comparisons on controlled EN/RU fixtures and publicly sourced representative
documents. Unexplained text loss, reading-order errors or invariant regressions
block adoption. Structural improvements need evidence and individual review.
macOS and Windows OCR smoke checks must pass before adoption; unavailable
platform checks remain explicit blockers rather than inferred acceptance.

## Rationale

A full merge keeps the fork maintainable while retaining its small OCR extension.
Build success alone cannot validate extraction changes; upstream modifies scan
classification, decoding, reading order and the OCR renderer's input. Public
fixtures make useful regression evidence reproducible without user documents.

See [the historical decision tree and corpus proposal](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pdf-inspector-upstream-sync.md).

## Implementation evidence

The merge preserved the fork's local OCR extensions; mdoc adopted tested fork
`1baba87892a929e64d08069cb223b4a312f755cf`. The [public corpus](../../tests/fixtures/ocr-upstream/README.md)
contains 12 public pages, plus native originals and synthetic EN/RU fixtures.
All 54 prepared/raw/provenance artifacts were byte-identical to baseline.
See [verification](../../tests/fixtures/ocr-upstream/verification.md) for checks
and pending Windows, remote-CI and external-suite acceptance. This is a regression
gate, not general OCR qualification.
