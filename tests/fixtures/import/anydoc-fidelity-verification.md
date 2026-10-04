# Selected AnyDoc fidelity update

Date: 2026-10-04. Baseline fork:
`391b6b109b21c051a0edd726755352d78df3853f`. Updated fork:
`97d21d1f46086d84478da072ba1fdfa210461c80`.

## Scope and integration

Ported only upstream PR #17 (`35f278e`), PR #177 (`27b3d78`, `2f0907d`), and
PR #174 (`96d5998`). All four commits were cherry-picked with author attribution
and source SHAs. There were no conflicts or functional changes to the submitted
patches. The only adaptation changes the new #17 snapshot to retain the current
base's inferred `Metric | Value` header instead of inserting an empty header.

No existing fork snapshots, mdoc expected outputs or transitive dependencies
were intentionally changed. mdoc's PDF path remains through its direct
pdf-inspector dependency; the fork's existing partial-PDF API is retained.

## Before/after evidence

The same converter harness ran against identical input bytes at both fork
revisions. All 75 pre-existing fork fixture outputs, including expected-error
diagnostics, were byte-identical. The new nested-table fixture also passes the
updated fork corpus. All 11 pre-existing mdoc import fixture outputs were
byte-identical through the AnyDoc comparison harness; three added fixtures show:

| Fixture | Before | After |
| --- | --- | --- |
| Nested Word table | One cell containing `Metric / Value`, `Height / 120`, `Weight / 34` | Separate 2-column table; the adjacent multi-cell wrapper retains its previous flattening |
| DOCX checkboxes | `Selected option`, `Unselected alternative`, and bold `Styled selection` | `☑ Selected option`, `□ Unselected alternative`, and bold `☑ Styled selection`; literal parentheses unchanged |
| DOC revisions | `Old clauseNew clause ~~Ordinary strike~~` | `New clause ~~Ordinary strike~~` |

These controlled fixtures establish the selected conversion behavior, not general
Office fidelity. The mdoc import tests additionally use its actual direct PDF
route and verify unchanged source bytes, metadata and warnings.

## Validation

- Baseline fork `cargo test --locked`: 301 passed, 1 ignored.
- Updated fork `cargo fmt --all --check`: passed.
- Updated fork `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  passed, including host builds of the Node, Python and wasm binding crates.
- Updated fork `cargo test --locked`: 318 passed, 1 ignored. Includes 11 DOC
  revision regressions, checkbox malformed/unknown-symbol cases, nested-table
  scope regressions, fixture snapshots, mutation robustness and partial-PDF tests.
- mdoc `cargo fmt --check`: passed.
- mdoc `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- mdoc `cargo test --locked -p mdoc import::tests`: 6 passed, including the three
  new fixtures through the existing representative-conversion regression.
- mdoc `cargo test --locked --workspace`: 461 passed, 11 ignored.
- mdoc `cargo-deny` 0.20.2 `check`: advisories, bans, licenses and sources passed.
  The pre-existing source allowlist omitted the app's already pinned
  `lawkitt/pdf-inspector` fork. Added that exact repository URL; no PDF revision or
  dependency policy was otherwise changed. The temporary check binary's archive
  was verified against its published SHA-256 and was not installed globally.
- Both repository diffs passed whitespace checks. The mdoc lockfile changes only
  AnyDoc's Git source revision; package versions and transitive edges are unchanged.

The fork review is [lawkitt/anydoc#1](https://github.com/lawkitt/anydoc/pull/1).
No remote fork checks were reported when queried; local validation above is the
available evidence. mdoc remote CI runs after its review PR is opened and is
separate from these completed local checks.

The ignored fork test requires an external local sample directory. No native UI,
Windows/Linux execution, binding-host runtime tests or wasm-target runtime tests
are claimed by these local converter checks. Remote CI is reported separately.
