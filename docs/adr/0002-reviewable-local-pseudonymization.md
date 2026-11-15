# Use reviewable local pseudonymization

Status: Accepted, 2026-10-01.

## Decision

- mdoc replaces identifying information locally with consistent placeholders
  that the user reviews. It is an assistant: automatic detection never
  establishes that text is anonymous.
- It reuses open-source libraries and models rather than growing a detection
  engine. The shipped integration is Rust, CPU only, with no bundled Python
  worker or Docker.
- The detector is GLiNER2 through gliner2-rs with a pinned PII export, run
  offline through the app's ONNX Runtime after explicit, verified setup.
- Placeholders are stable category tokens (`PERSON_1`, `ORG_1`).
- Generic annotations and painted geometry belong in `mdoc-editor`;
  detection, review policy, grouping and mappings belong in the app's crates.

## Invariants

- Pseudonymize is explicit; scanning never blocks normal editing or find.
- Apply is one undoable edit. Keep changes no text. A stale revision or
  offset never replaces text.
- Linking variants of a name into one identity needs a user action, except
  a form with exactly one candidate full name.
- The scan covers everything Copy Markdown transfers, hidden source
  included, and preserves Markdown syntax: protected syntax is never
  replaced. Original files and file names are outside its scope.
- Opening and editing documents never depend on a model.
- Without a saved case, mappings live only in the tab and end when it closes.

## Consequences

GLiNER2 ships as review assistance, not as a qualified detector; no candidate
passed the [initial qualification](../../tests/fixtures/pseudonymization/README.md).
The rules for detection and review are in the recognition and
cases-and-privacy specs.

References: [full text before the docs doctor](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0002-reviewable-local-pseudonymization.md),
[inline verification](../../tests/fixtures/pseudonymization/inline-review.md).
