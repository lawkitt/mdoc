# Remove Anonymization mode; Pseudonymization is the default

Status: Accepted, 2026-10-08. Supersedes
[ADR 0013](0013-reviewable-anonymization.md). Toolbar placement superseded by
[ADR 0048](0048-workspace-shell-redesign.md).

## Problem

Anonymize (shared category markers applied in one step) and Pseudonymize
(identity-linked aliases, explicit Apply) shared one review model through
`Mode` branches, marker-to-alias conversion, a split button and a second popup.

## Decision

- Pseudonymization is the only PII replacement behavior and the default.
- Pseudonymize scans and opens the Replacements panel with proposals; Apply is
  explicit. With a review and the panel closed it reopens without rescanning.
  ⌘⇧P (Ctrl+Shift+P) stays; the Anonymize action and ⌘⇧A are gone.
- One popup path; marker→alias conversion is deleted, so leftover markers are
  ordinary text.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/5cad017e5a38efe1f009d47f54d1f0d9e2995547/docs/adr/0022-remove-anonymization-mode.md).
