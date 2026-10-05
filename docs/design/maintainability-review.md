# Maintainability review — 2026-10-05

Status: design confirmed and first cleanup implemented locally on 2026-10-05.
The accepted scope and verification are recorded in
[ADR 0010](../adr/0010-workspace-construction-and-test-parity.md). The evidence
below describes the initial checkout; ADR 0010 records what changed afterward.

## Assessment

The codebase is reasonably maintainable at the application/service boundaries,
with concentrated debt in editor implementation and workspace/tab lifecycle
coordination. A broad rewrite is not justified by this investigation. Targeted
cleanup has concrete candidates; line count alone is not a reason to refactor.

Document identity/provenance, import completion, preview resources, search
scheduling, pseudonymization review, model installation and settings have named
owners. Existing lifecycle tests exercise cancellation, stale completions,
source preservation and retained tabs. Worker-owned permits protect native
runtime lifetimes after UI cancellation; these are justified safeguards, not
evidence of overengineering.

## Evidence and candidates

1. **Production versus standalone Workspace paths.** Production opens `Tabs`
   (`src/main.rs`, application entry point). `Tabs::initialize` constructs each
   `Workspace` with an owner, shared preferences/theme and Settings panel
   (`src/tabs.rs:512`). `src/ui_tests.rs:5` instead boots `Workspace::new`, a
   test-only constructor with no owner or model panel. Workspace methods retain
   standalone theme, document transition/close and direct OCR-install branches
   (`src/main.rs:288`, `379`, `425`, `675`). These are production-unreachable
   candidates under the current construction path, although tests exercise the
   standalone mode. Migrate relevant tests to the production shell or explicit
   dependencies before removing the fallback behavior. Do not simply delete it
   and discard coverage.
2. **Editor concentration.** `crates/mdoc-editor/src/lib.rs` has 7,350 lines,
   with the main test module starting at 6,800; `element.rs` has 4,698 lines,
   with its test module starting at 4,547. `EditorState` combines source,
   selection/IME, undo, painted geometry, menus, providers and caches. Input/edit
   and geometry responsibilities are plausible extraction seams. Keep one
   owner and current behavior initially; do not invent a generic rendering or
   plugin framework. An extraction needs a concrete editing/testing benefit.
3. **Tab lifecycle state.** `src/tabs.rs` has 1,862 lines, with tests in a
   separate 1,455-line file. It coordinates restoration/loading, conversion
   queues, settings, persistence, close/quit and sidebar rendering. Workspace
   exposes several lifecycle booleans that Tabs mutates directly. Consider
   grouping or explicit transitions only where they clarify a real invariant;
   this review does not establish a lifecycle bug or justify a wholesale state
   machine replacement.
4. **Retained reusable code.** The application consumes `mdoc-markdown` syntax
   through `mdoc-editor` with default features disabled. `cargo tree -p mdoc -e
   features -i mdoc-markdown --offline` confirms no reader `view` feature in this
   graph. Its 4,856-line reader module is retained repository/API surface, not
   shipped app reader code. `os-spellcheck` is used by the editor demo, rather
   than the app. Neither is proven dead repository code. Removing these would
   change the existing reusable-crate policy in ROADMAP.
5. **Small duplication.** OCR integrity checking (`src/ocr.rs:310`) and model
   artifact checking (`src/model_download.rs:35`) each stream SHA-256 and format
   its digest. Their validation/error contracts differ. Sharing a small hash
   helper is optional; a generalized artifact framework is not warranted by
   this duplication.

## Verification and limits

The initial checkout was clean. Current macOS `cargo clippy --workspace
--all-targets --offline -- -D warnings` passes. The emitted warnings concern a
Cargo registry setting and an upstream `block` future-compatibility report,
not application Clippy/dead-code diagnostics. No `allow/expect(dead_code)` or
`allow/expect(unused...)` suppressions were found in the examined app, local
crates or tooling Rust files. Public unused library APIs and runtime-unreachable
branches are not established as live by Clippy.

`cargo machete` is not installed, so no automated unused-dependency result is
claimed. No test suite, benchmark or native UI run was performed for this
read-only code assessment. Vendored GPUI and dependency forks were not deeply
audited. These findings do not establish cross-platform acceptance or a complete
absence of dead code.

## Decision tree

- Round 1 — both confirmed by the user's "agree":
  - Q1: Address targeted maintenance debt now or defer until a related change?
    Decision: a small cleanup beginning with production/standalone Workspace
    divergence. Consider one editor extraction only with a concrete benefit.
  - Q2: Preserve the retained reusable-crate/feature surface or audit removals?
    Decision: preserve it; unused by mdoc is not sufficient for deletion.
- Round 2 — Q3 confirmed by the user's "Ok": approve the first change specified
  in ADR 0010 and confirm shared understanding. Decision: limit it to Workspace construction,
  standalone fallback removal and production-path test migration, with the
  stated behavior invariants and verification. Defer editor extraction and
  broader lifecycle restructuring until this change is complete and assessed.
- The frontier is empty for this first change. Shared understanding was confirmed;
  implementation and the available verification are complete. Editor extraction
  and broader restructuring remain deferred.

No terminology decision requires a new glossary entry. Existing CONTEXT terms
and ROADMAP source/behavior/reusable-crate constraints remain the reference.
