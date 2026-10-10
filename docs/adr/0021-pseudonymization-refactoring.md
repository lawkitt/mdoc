# Behavior-preserving pseudonymization refactoring

Status: accepted and implemented, 2026-10-08. The user confirmed the interview and requested full implementation; see the
[historical design interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-refactoring.md).

## Problem

ADRs 0013, 0014, 0018, 0019 and 0020 were implemented in rapid iterations without
consolidation. About 10k lines now live in three crate-root trees. All UI logic is
`impl Workspace`, state resets are copied by hand, `Review` mixes candidates,
identities, tracking and mode, Anonymize/Pseudonymize interplay is spread through
a `conversion` flag, and identifiers use several vocabularies.

## Decision

Refactor without changing behavior. The existing suite and strict Clippy gate
every commit; any behavior change requires its own ADR.

- Delete code of superseded interaction designs once verified unreachable.
- Keep `Workspace` as gpui glue; move UI state and transitions into plain structs.
- Move the domain model into `crates/mdoc-pii`; the detector stays for now.
- Treat Anonymize mode as permanent: a rendering policy over the same identities.
- Replace plan/target tuples with named structs; centralize identity-history
  transactions in one helper.
- Use `pii` as the umbrella module/action vocabulary with CONTEXT.md glossary
  terms inside.
- Move run evidence out of `tests/fixtures` to `docs/evidence/pseudonymization/`,
  retaining per-run READMEs.
- Deliver as incremental green commits, renames last.

## Invariants

Stable aliases, occurrence provenance, live-tab-only originals, Keep/Restore
scopes, undo/redo travel, syntax protection, stale-result guards, exact Copy
Markdown and experimental status remain unchanged.

## Implementation

Implemented in six green commits; see the design interview's implementation
section for the per-step record, decisions taken during implementation and
verification. Notable structure:

- `crates/mdoc-pii`: GUI-free review model (`Review` façade over `Candidates`,
  `IdentityStore`, `Tracking`); depends only on `aho-corasick`, `regex` and
  `crates/mdoc-history`.
- `crates/mdoc-history`: dependency-free edit transactions, re-exported by
  `mdoc-editor` at their existing paths.
- `src/pii/detector*` and `src/pii/ui*`: detection and the review workspace;
  `MappingUi`/`ReviewUi` own every panel and popup state transition.
