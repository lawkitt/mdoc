# Review upstream converter updates against preserved fork behavior

Status: Accepted, 2026-10-04.

## Decision

Take upstream pdf-inspector updates as complete merges into the fork, keeping
its history and the local OCR extensions: pinned Cyrillic manifests, explicit
runtime paths, offline recognition, explicit app consent and optional
recognition output before fusion. mdoc pins the exact tested fork revision.

An update is adopted only after fork checks with OCR enabled, mdoc checks, and
before/after output comparison on controlled EN/RU fixtures and the
[public corpus](../../tests/fixtures/ocr-upstream/README.md). Unexplained text
loss, reading-order errors or invariant regressions block adoption; structural
changes need evidence and individual review. macOS and Windows OCR smoke checks
must pass; an unavailable platform check is a stated blocker, not inferred
acceptance.

## Rationale

A full merge keeps the fork maintainable. Build success cannot validate
extraction changes, and public fixtures make regressions reproducible without
user documents. This is a regression gate, not general OCR qualification.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/5cad017e5a38efe1f009d47f54d1f0d9e2995547/docs/adr/0007-review-upstream-converter-updates.md),
[decision tree](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pdf-inspector-upstream-sync.md),
[verification](../../tests/fixtures/ocr-upstream/verification.md).
