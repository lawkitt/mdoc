# App architecture and code ownership

Status: Accepted, 2026-10-25.

## Decision

- **Goal:** a feature change touches one obvious place, builds and tests stay
  fast, and every state machine has an explicit owner. File size alone never
  justifies a split.
- **Crates by dependency weight.** A new workspace crate is allowed when it
  gives a GUI-free boundary; never a service container, trait hierarchy, DI
  or framework.
  - `mdoc-store`: app paths, documents and atomic saves, the session, recents,
    settings, case storage and encryption, crash-report masking.
  - `mdoc-import`: import, OCR, recognition, model download, the DOCX preview
    worker.
  - `mdoc-detect`: the detector and its rules. `mdoc-pii` stays light and
    platform-neutral.
  Settings hold plain model IDs; manifest lookups live with their model's
  crate. Store types name no GPUI or app-style types.
- **App layout.** One folder per owner (`src/workspace/`, `src/document_view/`,
  `src/pii/ui/`). Each feature declares its own `actions!` (all in the `mdoc`
  namespace) and keybindings. The top level holds only startup and the shared
  UI kit.
- **Owners.** A state machine is a plain struct with private fields whose
  named transitions are synchronous, GPUI-free and return the owner's own
  effect enum; the GPUI view carries effects out (tasks, prompts, events). A
  child GPUI entity is used only for a region that renders and takes focus on
  its own. Owners are named for what they own (no Manager, Controller or
  Service). New state goes into an owner, not onto `Workspace` or
  `DocumentView`, unless it is view state.
- **Errors:** new crates return typed (`thiserror`) errors; user text is made
  at the UI edge.
- **One channel each way.** A document reaches the window only through
  `TabEvent`s grouped by owner; the window commands a document through its
  methods. A document holds no handle to its window.
- **Tests by owner.** Plain owners get GPUI-free unit tests beside them;
  headless flow tests cover owner interplay under `<owner>/tests/`; shared
  fixtures live in one `test_support` module.

## Invariants

- `just lint` enforces `clippy::wildcard_imports` (except GPUI's prelude and
  `use super::*` in tests), no `#[path]` under `src/`, and no `gpui` in the
  dependency tree of `mdoc-store`, `mdoc-import`, `mdoc-detect`, `mdoc-pii`,
  `mdoc-history`, `mdoc-spell` or `mdoc-markdown`.
- A refactor preserves behaviour: no user-observable change, and tests are
  adapted, never removed.
- A refactor touching render or state paths runs
  `MDOC_PERF_PROFILE=release just perf` before and after and raises no
  ceiling, and compares `just headless-renders` pixel for pixel, explaining
  every difference.

References: [full text before the docs doctor](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0069-app-architecture-and-code-ownership.md),
[interview](https://github.com/lawkitt/mdoc/blob/cb72c2fa853cd554b462e27f1375640b3392e035/docs/design/codebase-refactoring.md).
