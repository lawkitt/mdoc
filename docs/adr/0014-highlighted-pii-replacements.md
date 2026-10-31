# Keep applied PII replacements inspectable and restorable

Status: Accepted, 2026-10-07. Superseded in part by
[ADR 0022](0022-remove-anonymization-mode.md) (no shared markers),
[ADR 0023](0023-pseudonymization-ui-polish.md) (Keep original replaces Restore)
[ADR 0041](0041-cases-and-ai-response-restoration.md) (cases store originals)
and [ADR 0088](0088-phone-and-iban-rules.md) (phone and IBAN rules).

## Problem

A token cannot tell which original belongs to a particular occurrence, so
restoring an applied replacement needs explicit provenance, protected against
stale edits and history.

## Decision

- Applied replacements stay highlighted and clickable after review closes. The
  user can revert one occurrence or all occurrences of the same original;
  reverting records Keep at that scope so later scans respect it.
- Originals and occurrence provenance live with the document across tab
  switches and Save. Without a case they are never written to Markdown, a
  sidecar or the session, and end when the tab closes.
- Unrelated edits shift tracked occurrences. Editing or deleting an alias
  removes its provenance. Undo/redo restores text, provenance and Keep decisions
  together; each revert is one undo step. Pasted lookalikes gain no provenance.
- Reverting returns the immediate pre-replacement text.

## Architecture

- One app-owned PII domain model (`crates/mdoc-pii`) with stable group,
  occurrence and provenance IDs, source-sorted spans and shared immutable
  before/after text. The generic editor owns annotation geometry, exact source
  transactions and history identities; PII policy stays in the app.
- Compact reversible metadata deltas follow editor undo units, coalescing,
  branching and pruning; no duplicate document-snapshot history.
- Source-sorted occurrence storage rebases in one ordered pass per edit batch,
  until measurements justify an offset tree. Exact originals use cached
  multi-pattern matching with background discovery accepted at the current
  revision. Paint visible rows plus overscan; dense hidden fields aggregate
  behind a counted gutter marker with keyboard navigation.
- The stress workload (2 MiB, 20,000 occurrences) is a benchmark, not a cap.

## Structured rules

Besides GLiNER2, the detector adds email and contextual checksum-valid
INN/SNILS rules (see [ADR 0017](0017-selected-rust-presidio-reuse.md)). Phone
and bank rules and full PII engines stay deferred: on the synthetic corpus they
added no exact hits.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0014-highlighted-pii-replacements.md),
[design tree](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/anonymizer-restoration.md),
[OSS research](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pii-oss-research.md),
[hybrid measurements](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/evidence/pseudonymization/2026-10-07-hybrid/measurements.md),
[restoration report](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/evidence/pseudonymization/2026-10-07-restoration/README.md).
