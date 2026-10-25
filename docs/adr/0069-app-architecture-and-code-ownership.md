# App architecture and code ownership

Status: Accepted, 2026-10-25. Amends [ADR 0010](0010-workspace-construction-and-test-parity.md)
(new crates) and [ADR 0049](0049-measured-stabilization-and-architecture-review.md)
(defines its case-coordination and document-lifecycle projects).

## Problem

After 0.1.0 the app crate grew to ~52k lines in a flat `src/` of 46 modules.
`Workspace` holds ~90 fields (focus, sidebar animation, quit, session writes,
DOCX queue, cases); `DocumentView` holds 77, all `pub(crate)`, extended by
`impl DocumentView` blocks in 22 files. 33 modules import `main.rs` through
`use super::*;`, eight files hide their place in the tree behind `#[path]`,
and documents reach the window both through `TabEvent` and a
`WeakEntity<Workspace>` back-reference. GUI-free logic (documents, session,
settings, cases, import, OCR, detection) builds inside the binary, so any
edit rebuilds all of it and none of it runs with the platform-neutral crates.
Two ADRs shared the number 0053.

## Decision

- **Goals:** a feature change touches one obvious place; faster builds and
  tests; a tree humans and agents can navigate; state machines with explicit
  owners. Behaviour is preserved under ADRs 0029 and 0049: no
  user-observable change, tests adapted rather than removed. File size alone
  still never justifies a split.
- **Crates by dependency weight.** A new workspace crate is allowed when it
  gives a GUI-free boundary; still no service container, trait hierarchy, DI
  or framework.
  - `mdoc-store`: app paths, `Document` and atomic saves, document session,
    session store, recents, settings types, case storage and encryption,
    crash-report masking.
  - `mdoc-import`: import and its sessions, OCR, recognition, model
    download, the DOCX preview worker.
  - `mdoc-detect`: the pseudonymization detector and its structured rules.
    `mdoc-pii` stays light and platform-neutral.
  Settings hold plain model IDs; manifest lookups live with their model's
  crate. Store types name no GPUI or app-style types.
- **App layout.** One folder per owner (`src/workspace/`,
  `src/document_view/`, `src/pii/ui/`); no `#[path]`; no glob imports except
  GPUI's trait prelude (`gpui::prelude::*`) and `use super::*` in test
  modules.
  Each feature declares its own `actions!` (all in the `mdoc` namespace, so
  action names don't change) and keybindings. The top level holds
  only startup and the shared UI kit.
- **Owners.** A state machine is a plain struct with private fields whose
  named transitions are synchronous, GPUI-free and return that owner's own
  effect enum; the GPUI view carries effects out (tasks, prompts, events).
  A child GPUI entity is used only for a region that renders and takes
  focus on its own. Owners are named for what they own (no Manager,
  Controller or Service). New crates return typed (`thiserror`) errors;
  user text is made at the UI edge.
  - Workspace: `TabSet`, `Sidebar`, `QuitFlow`, `SessionWriter`,
    `DocxQueue`, `CaseCoordinator` (ADR 0049 case coordination),
    `FocusRegions`, `Notices`.
  - DocumentView: `DocumentLifecycle` (ADR 0049 document lifecycle),
    `ImportFlow`, `CaseLink`, beside the existing `ReviewUi`, `SpellState`,
    `PreviewState` and `SearchSession`.
  - Fields are private; other modules use owner methods. New state goes into
    an owner, not onto `Workspace` or `DocumentView`, unless it is view
    state.
- **One channel each way.** A document reaches the window only by emitting
  `TabEvent`s grouped by owner; the window commands a document through its
  methods. A document holds no handle to its window; what it must read is
  passed in or shared read-only.
- **Tests by owner.** Plain owners get GPUI-free unit tests beside them.
  Headless flow tests cover owner interplay, split by owner under
  `<owner>/tests/`. A flow test becomes a unit test only when it checks
  nothing beyond one transition. Shared fixtures live in one `test_support`
  module.
- **Delivery.** Small PRs in phases, each passing `just check` and reviewed
  by Codex before it opens. Feature work and bug fixes are frozen on `main`
  until P5 ends; 0.2.0 ships from rc.3 if it passes acceptance, otherwise the
  next release is cut from `main` after P5.
  - P1: explicit imports and per-feature actions; folders, no `#[path]`.
  - P2: `mdoc-store`, `mdoc-detect`, `mdoc-import`; each also built on
    Windows before merge.
  - P3: `Notices`, `FocusRegions`, `Sidebar`, `TabSet`; `SessionWriter`,
    `DocxQueue`, `QuitFlow`; last `CaseCoordinator` with the one-channel
    change.
  - P4: `ImportFlow`, `DocumentLifecycle`, `CaseLink`; private fields on the
    existing owners; tests split with each owner.
  - P5: `mdoc-editor` and `gpui-pdf` module boundaries and private fields
    only; ADR 0011 holds.
  - `scripts/` is a separate later item; `tools/pseudonymization` stays out.
- **Boundaries before code.** Each P3/P4 PR opens with a boundary note:
  owned state, transitions, effects, kept invariants, handled events. On
  merge, the owner list below gains one line. A separate ADR is written only
  when an extraction changes a rule in force.
- **ADR 0053's second ADR** became ADR 0068: the one renumbering, because
  the number was never unique.

## Invariants

All invariants of ADRs 0010, 0011, 0021, 0029 and 0049 hold. P3/P4 PRs that
touch render or state paths run `MDOC_PERF_PROFILE=release just perf` before
and after and raise no ceiling. Each phase ends with `just headless-renders`
compared pixel for pixel against the pre-phase output; every difference is
explained. After P4 the next RC runs full native platform acceptance on both
shipped targets. `just lint` enforces `clippy::wildcard_imports`, no
`#[path]` under `src/`, and no `gpui` in the dependency tree of `mdoc-store`,
`mdoc-import`, `mdoc-detect`, `mdoc-pii`, `mdoc-history` or `mdoc-spell`,
each from the phase that makes it true.

## Consequences

- development.md's layout table is rewritten as each phase moves code.
- CONTEXT.md defines owner, transition and effect.

Owners extracted so far:

- `Notices` (`src/workspace/notices.rs`): the session message and last
  run's crash report behind the header bell, and when the bell opens.

References: [interview](../design/codebase-refactoring.md).
