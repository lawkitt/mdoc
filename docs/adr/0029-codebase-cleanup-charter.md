# Codebase cleanup charter

Status: accepted and implemented, 2026-10-09. See the
[historical design interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/codebase-cleanup.md). Supersedes the "preserve
reusable crates/features" scope of [ADR 0010](0010-workspace-construction-and-test-parity.md)
and the deferral of API/feature removal in [ADR 0011](0011-editor-input-module.md).

## Decision

- mdoc is an application, not a library family. Code in the workspace crates
  that the mdoc binary cannot reach is dead unless a named consumer exists.
- The cleanup is behaviour-preserving for the shipped app. Removing unreachable
  code is not a behaviour change; removing anything a user can observe today
  requires its own decision.
- Tests are removed or merged only when they exercise removed code, fully
  duplicate another test, assert implementation detail, or cannot fail. Length is
  not a reason. Long scenarios are split only when they chain unrelated
  behaviours and failures would be hard to localize.
- File size alone does not justify a split.
- `tools/pseudonymization` is out of scope.

## Consequences settled so far

- `mdoc-markdown`: the unused `view` reader is deleted; `syntax` moves into
  `mdoc-editor` and the crate is removed.
- Editor host hooks the app never sets are deleted with their set-only code
  paths; the unset path renders identically.
- `os-spellcheck` is retained as groundwork for a deferred spell-check
  experiment (ROADMAP) — the one named exception to the dead-code rule.
- `gpui-pdf` features become unconditional; its examples are deleted.
- Crates are internal: no `API.md`, short READMEs, `publish = false`,
  LICENSE files kept.
- Zorite note syntax stops rendering ([ADR 0030](0030-render-standard-markdown-only.md)).
- Test removals are listed with reasons and approved before deletion.

## Invariants

All invariants of ADRs 0010, 0011 and 0021 hold. Every commit passes
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo test --workspace`.
