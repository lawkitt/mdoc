# Behavior-preserving pseudonymization refactoring

Status: Accepted, 2026-10-08.

## Problem

ADRs 0013, 0014, 0018, 0019 and 0020 were built in rapid iterations: about 10k
lines in three crate-root trees, all UI logic in `impl Workspace`, hand-copied
state resets, a `Review` mixing candidates, identities, tracking and mode, and
several vocabularies for the same things.

## Decision

Refactor without changing behavior; any behavior change needs its own ADR (the
one exception was [ADR 0022](0022-remove-anonymization-mode.md)).

- Delete code of superseded interaction designs once verified unreachable.
- `Workspace` is GPUI glue; UI state and transitions live in plain structs
  (`MappingUi`, `ReviewUi` own every panel and popup transition).
- `crates/mdoc-pii` holds the GUI-free review model (`Review` over `Candidates`,
  `IdentityStore`, `Tracking`), depending only on `aho-corasick`, `regex` and
  `crates/mdoc-history`. `crates/mdoc-history` holds dependency-free edit
  transactions, re-exported by `mdoc-editor`. Detection and the review UI live
  in `src/pii/`.
- Named structs replace plan/target tuples; identity-history transactions go
  through one helper.
- `pii` is the umbrella module and action vocabulary, with CONTEXT.md terms.
- Work lands as incremental green commits, renames last.

## Invariants

Stable aliases, occurrence provenance, Keep/revert scopes, undo/redo, syntax
protection, stale-result guards and exact Copy Markdown are unchanged; the suite
and strict Clippy pass at every commit.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0021-pseudonymization-refactoring.md),
[interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-refactoring.md).
