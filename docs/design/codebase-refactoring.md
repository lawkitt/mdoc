# Codebase refactoring interview (2026-10)

Working record of the refactoring interview. Per ADR 0052 it leaves the tree
once its decisions live in an ADR, ROADMAP and CONTEXT.md; link it by permalink.

Baseline: `main` at a20e3e3 (v0.2.0-rc.3), 174 commits since the last
refactoring commit (ef06079, 2026-10-16).

## Findings (facts, not decisions)

### Size

- `src/` (the binary crate): ~52k lines, 46 top-level modules in a flat
  directory. Workspace crates: mdoc-editor 22.4k, gpui-pdf 5.3k, mdoc-pii 4.9k,
  gpui-bidi 1.2k, mdoc-spell 0.9k, mdoc-history 41 lines.
- Largest app files: `workspace_tests.rs` 6.3k, `workspace.rs` 3.6k,
  `pii/ui/mapping_tests.rs` 2.6k, `document_view_tests.rs` 2.5k,
  `document_view.rs` 2.4k, `pii/ui/tests.rs` 2.0k, `workspace_cases.rs` 1.9k,
  `workspace_shell.rs` 1.8k.

### Structure

- **Two god objects.** `Workspace` (window shell) has ~90 fields mixing focus
  handles, sidebar/reveal animation, file-drop state, the quit state machine,
  session persistence, the DOCX job queue, case state, theme and notices.
  `DocumentView` has ~77 fields; 22 files add `impl DocumentView` blocks
  (all of `pii/ui/*`, spelling, preview, search_session, chrome).
- **Features as impl blocks.** Features attach methods to the god objects
  rather than owning state behind a boundary; `ReviewUi` and `ShellState` are
  partial exceptions.
- **`#[path]` aliasing.** `workspace_cases.rs`, `workspace_shell.rs`,
  `workspace_keychain.rs`, `workspace_actions.rs`, `headless_renders.rs`,
  `workspace_tests.rs`, `docx_comments.rs`, `mapping_tests.rs` are submodules
  that look like top-level files.
- **Glob prelude.** 33 modules start with `use super::*;`, inheriting
  `main.rs`'s imports and its ~100 `actions!`; dependencies between modules are
  invisible.
- **Event bus.** `TabEvent` (17 variants) carries document → window requests,
  including case work (CaseChanged, AddCaseTerm, CreateCase, MappingMissing).
- **GUI-free logic inside the binary.** cases, settings, session_store,
  document, document_session, import, import_session, recognition, comparison,
  model_download, recents, docx_preview, crash have 0–5 GPUI references each,
  but live in the bin crate, so they build with the app and can't be tested
  on `wsl` with the platform-neutral crates.
- Only the binary has a `main.rs`; no `lib.rs`.

### Process context

- ADR 0010: no trait hierarchy, service container or new crate (for workspace
  construction). ADR 0029: behaviour-preserving cleanup; file size alone
  doesn't justify a split. ADR 0049: case coordination and document lifecycle
  are separate incremental ownership projects, boundaries defined first; no
  service framework or text-storage replacement.
- Cold `just lint` + `just test` ≈ 4.5 min on `windows`; a worktree's
  `target/` ≈ 30 GiB.
- Two ADRs share number 0053 (`cargo-lock-matches-manifest`,
  `short-and-inflected-name-variants`); code cites "ADR 0053" for both.

- `DocumentView`'s 77 fields are all `pub(crate)`, as are most of
  `ReviewUi`'s: any module can write any document state.
- GUI-free module dependencies: `settings` → ocr manifest, pii detector
  manifest, recognition, model_download; `cases` → `pii::Category`,
  `pii::restore::ExportedIdentity` and one `gpui::hsla` colour;
  `session_store` → `style::ThemeChoice`; `comparison` → ocr/pii setup. The
  pii detector (`src/pii/detector`) carries the heavy ONNX/GLiNER2 deps;
  `ocr`/`import` carry anydoc and pdf-inspector.

## Decisions

### Round 1

- **D1 Goals:** all four, without ranking: change locality (a feature change
  touches one obvious place), build/test time, navigability for humans and
  agents, and correctness of state machines through explicit ownership.
- **D2 Behaviour-preserving.** The ADR 0029/0049 rules hold: no
  user-observable change; tests are adapted, not removed. A visible change
  needs its own issue or ADR.
- **D3 New GUI-free crates allowed.** A new workspace crate is allowed when it
  gives a GUI-free boundary. Still no service container, trait hierarchy, DI
  or framework. Amends ADR 0010's "no new crate".
- **D4 Delivery:** incremental PRs in a sequence agreed here, each passing
  `just check` and preserving behaviour. Feature work is frozen until the
  refactoring ends (amended in round 5, D21).
- **D5 File size** alone still never justifies a split; splits follow
  ownership boundaries decided here.
- **D6 ADR 0053 duplicate:** `0053-short-and-inflected-name-variants` becomes
  ADR 0068 and its citations (CONTEXT.md, `identities.rs`, ADRs 0018, 0056)
  are updated. The one exception to never-renumber, because the number was
  never unique.

### Round 2

- **D7 Crates by dependency weight.** New GUI-free crates:
  - `mdoc-store`: app paths, `Document` and atomic saves, document session,
    session store, recents, settings types, case storage/encryption, crash
    masking. Light deps.
  - `mdoc-import`: import and import session, OCR, recognition, model
    download/work, DOCX preview worker. Heavy deps (anydoc, pdf-inspector,
    ort).
  - `mdoc-detect`: the pii detector (GLiNER2/ONNX) and its structured rules;
    `mdoc-pii` stays light and platform-neutral.
  Settings hold plain model IDs; manifest lookups move into `mdoc-import` /
  `mdoc-detect`. Cases return a hue number, not `gpui::Hsla`.
  `session_store` stops naming `style::ThemeChoice` (a store-side enum).
- **D8 Folders per owner.** `src/workspace/…`, `src/document_view/…`,
  `src/pii/ui/…`; no `#[path]`; tests in `<owner>/tests.rs` (or a `tests/`
  subfolder). Top level holds only startup and shared UI kit.
- **D9 No glob imports.** All `use super::*;` go; imports are explicit. Each
  feature declares its own `actions!` and keybindings.
- **D10 Owner pattern (mixed).** State machines are plain structs with
  private fields and named transitions that return effects for the GPUI owner
  to carry out (the `Review` pattern). Child GPUI entities only for regions
  that render and take focus independently.
- **D11 Owners.**
  - Workspace: `TabSet`, `Sidebar`, `QuitFlow`, `SessionWriter`, `DocxQueue`,
    `CaseCoordinator` (ADR 0049 case coordination), `FocusRegions`, `Notices`.
  - DocumentView: `DocumentLifecycle` (ADR 0049 document lifecycle),
    `ImportFlow`, `CaseLink`; existing `ReviewUi`, `SpellState`,
    `PreviewState`, `SearchSession` stay.
  - Fields are private everywhere; other modules use owner methods.

### Round 3

- **D12 One channel each way.** A document talks to the window only by
  emitting `TabEvent`s grouped by owner (`Case(…)`, `Import(…)`, `Close(…)`
  …); the window commands a document by calling its methods. The
  `owner: (u64, WeakEntity<Workspace>)` back-reference goes; what a document
  must read (other open paths, close in progress) is passed in or shared as a
  small read-only value.
- **D13 Tests by owner.** Plain-struct owners get GPUI-free unit tests beside
  them. Headless flow tests stay for owner interplay, split into
  `workspace/tests/{tabs,quit,session,cases,docx,sidebar}.rs` and similar.
  A flow test becomes a unit test only if it checks nothing beyond one
  transition. Shared fixtures go to one `test_support` module. ADR 0029 test
  rules apply.
- **D14 Scope.** `src/`, the new crates, and as the last phase `mdoc-editor`
  and `gpui-pdf` for module boundaries and private fields only (no
  architecture change; ADR 0011 holds). `scripts/` refactoring is planned as
  a later, separate item (ROADMAP Later). `tools/pseudonymization` stays out.
- **D15 Guardrails.** `clippy::wildcard_imports = "deny"` in
  `[workspace.lints]`; `just lint` fails on `#[path]` in `src/`; `just lint`
  checks with `cargo tree` that `mdoc-store`, `mdoc-import`, `mdoc-detect`,
  `mdoc-pii`, `mdoc-history`, `mdoc-spell` never depend on `gpui`;
  development.md states that new state goes into an owner, not a new field
  on `Workspace`/`DocumentView` unless it is view state.
- **D16 Records.** ADR 0069 "App architecture and code ownership" holds the
  rules; it amends ADR 0010 (new crates) and ADR 0049 (its two projects).
  development.md's layout table is rewritten after the moves; CONTEXT.md
  gets a "Code structure" section (owner, transition, effect, owner names);
  ROADMAP's Architecture item points at ADR 0069 and the PR sequence. This
  record is permalinked and removed once those exist.

### Round 4

- **D17 Start now on `main`**, while 0.2.0 is in its RC cycle (D21).
- **D18 Phases.**
  - P1 Mechanical: remove globs + per-feature actions (D9) with the
    wildcard lint; folders per owner, no `#[path]` (D8) with its check. Land
    fast.
  - P2 Crates: `mdoc-store`, `mdoc-detect`, `mdoc-import` (D7) with the
    GPUI-free check; the Windows build checked on a `windows` lane before
    merge.
  - P3 Workspace owners: `Notices`, `FocusRegions`, `Sidebar`, `TabSet`;
    then `SessionWriter`, `DocxQueue`, `QuitFlow`; last `CaseCoordinator`
    with the one-channel change (D12).
  - P4 DocumentView owners: `ImportFlow`, `DocumentLifecycle`, `CaseLink`;
    private fields on existing owners; test split (D13) with each owner.
  - P5 `mdoc-editor`/`gpui-pdf` module boundaries and private fields (D14).
  - Later: `scripts/`.
- **D19 Verification.** Every PR: `just check` and a Codex review of the diff
  before the PR opens. P3/P4 PRs touching render or state paths:
  `MDOC_PERF_PROFILE=release just perf` before and after, no raised ceiling.
  Each phase end: `just headless-renders` compared pixel-for-pixel with the
  pre-phase baseline; every difference explained. After P4 the next RC runs
  full native platform acceptance on both platforms.
- **D20 Conventions.** One effect enum per owner; transitions synchronous and
  GPUI-free; new crates use typed `thiserror` errors, converted to user text
  at the UI edge; owners named for what they own (no Manager/Controller/
  Service).

### Round 5

- **D21 Feature freeze.** No feature PRs land on `main` from the first
  refactoring PR until P5 is done; only the refactoring lands. Replaces the
  release-branch option.
- **D22 Boundary notes.** Each P3/P4 PR description opens with a boundary
  note: owned state, transitions, effects, kept invariants, handled events.
  On merge, a one-line summary joins ADR 0069's owner list. A separate ADR
  only when an extraction changes a rule in force.
- **D23 Full freeze.** Bug fixes also wait until the refactoring ends; there
  is no `release/0.2` branch. If rc.3 passes the Windows run, 0.2.0 ships
  from its tag. Otherwise, and for any 0.2.x fix, the next release is cut from
  `main` after P5, with full native acceptance on both platforms (D19).
