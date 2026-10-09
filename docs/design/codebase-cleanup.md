# Codebase cleanup — 2026-10-09

Status: interview in progress. Not authorized for implementation until the user
confirms shared understanding. Decisions are recorded in
[ADR 0029](../adr/0029-codebase-cleanup-charter.md) as they are settled.

## Evidence (survey, 2026-10-09)

- ~68k lines of Rust in the app and seven workspace crates; ~600 tests.
- `mdoc-markdown`'s `view` feature (`view.rs`, 4,856 lines) is never enabled by
  the app: `mdoc-editor` depends on it with `default-features = false`.
- `os-spellcheck` is used only by the `mdoc-editor` demo example.
- `mdoc-editor` public API the app never calls: math (`set_block_math_provider`,
  inline/block math editing), Mermaid, embeds, code highlighting/languages,
  spell-check (`on_suggest`, `set_diagnostics`, `Diagnostic`), auto-replace,
  clipboard writer, scroll compensator, labels, tab indent, grip inset,
  property icons, block-ref counts, alert icons, `CellAlign`, `MathAlign`.
- Zorite syntax still renders in mdoc today: `[[wiki]]` and `#tag` render as
  links whose `OpenWikiLink` event the app ignores; `key:: value` lines render as
  property pill panels; `((id))` block refs and `![[embed]]` lines are recognized.
- `Workspace` (`src/main.rs`) is the per-document view; `tabs::Tabs` is the
  window shell.
- App UI tests are long multi-behaviour scenarios; perf/model tests are
  `#[ignore]`d manual runs.
- Hygiene: stale Zorite `.gitignore` entries; `.agents/skills/grilling`
  duplicates `.claude/skills/grilling`; dangling `TODO.md`
  (`view.rs:2065`) and `docs/design/maintainability-review.md` (ADR 0011)
  references.

## Round 1 — accepted as recommended ("Agree")

1. Strictly behaviour-preserving for the shipped app; any user-visible change
   needs its own ADR. Removing unreachable features is not a behaviour change.
2. Reverse ADR 0010's "preserve reusable crates/features": code unreachable from
   the mdoc binary is dead unless a named consumer exists. No other consumer is
   known.
3. A test is removed or merged when it (i) only exercises removed code,
   (ii) duplicates another test's behaviour fully, (iii) asserts implementation
   detail rather than observable behaviour, or (iv) cannot fail in practice.
   Length alone is not a reason. Invariant coverage from ADRs 0010/0011/0021
   must not drop.
4. Split scenario tests only where they chain unrelated behaviours and failure
   would be hard to localize; extract shared setup helpers.
5. Rename `Workspace` → `DocumentView` and `Tabs` → `Workspace`; move the
   document view out of `main.rs`. Renames land last; update CONTEXT.md.
6. No splits for file size alone; revisit after dead-code removal and split only
   along clear cohesion lines.
7. Fix hygiene items; keep design docs as history, marking superseded ones in
   their headers.
8. `tools/pseudonymization` is out of scope.
9. Small green commits (fmt, clippy `-D warnings`, workspace tests), ordered
   dead code → tests → renames; one local unused-dependency scan, not in CI.

## Round 2 — answered 2026-10-09

1. Delete `mdoc-markdown`'s `view` feature (with its tests, the `markdown`
   dependency and its API docs); move `syntax` into `mdoc-editor` and delete
   the crate. *Agreed.*
2. **Keep `os-spellcheck`** for a later spell-check experiment (added to
   ROADMAP "Deferred"). *User override of the recommendation.* What else it
   keeps is settled in round 3.
3. Delete editor host hooks the app never sets, with code that runs only when
   they are set; the unset path must render identically. *Agreed* (scope
   relative to spell-check: round 3).
4. Stop rendering Zorite syntax: `[[wiki]]`/`#tag` links, `key:: value`
   property pills, `((id))` block refs and `![[embed]]` lines become plain
   Markdown text. A visible change, recorded separately as ADR 0030. Saved bytes
   and Copy Markdown are unchanged. *Agreed.* `<mark>`/`<span>` styled tags stay.
5. Make `gpui-pdf`'s `markup`, `search` and `forms` unconditional; delete its
   three examples. *Agreed.*
6. Delete per-crate `API.md`; trim READMEs to purpose/ownership; drop
   crates.io metadata and add `publish = false`; keep LICENSE files. *Agreed.*
7. Test removals: list candidates with reason and covering test in this doc,
   get approval, then implement. *Agreed.*

## Round 3 — answered 2026-10-09

1. Spell-check groundwork kept whole: `os-spellcheck`, the editor's
   `Diagnostic`/`set_diagnostics`/`on_suggest` hooks with underline and
   suggestion menu (exempt from round 2 Q3), and the `mdoc-editor` demo trimmed
   to spell-check wiring so `clippy --all-targets` keeps them compiled. *Agreed.*
2. Renames: `Workspace` → `DocumentView` (`src/document_view.rs`;
   `workspace_ui.rs` → `src/document_view/chrome.rs`);
   `WorkspaceDependencies` → `DocumentViewDependencies`; `tabs::Tabs` →
   `workspace::Workspace` (`src/workspace.rs`); `tabs_tests.rs` →
   `workspace_tests.rs`; `ui_tests.rs` → `document_view_tests.rs`; `Tab` and
   `TabEvent` keep their names. Glossary terms added to CONTEXT.md. *Agreed.*
3. `.agents/skills/grilling` and `.claude/skills/grilling` both stay as they
   are. *User override of the symlink recommendation.*
4. Tests that only exercise deleted code go in the same commit, named in its
   message. The approval list covers criteria (ii)–(iv) and is written after the
   dead-code commits. *Agreed.*

## Defaults applied without a separate question

- Existing `#[allow(clippy::…)]` attributes stay unless their function is
  touched anyway; no signature redesign to satisfy them.
- `gpui-pdf`'s deliberate copy of `is_safe_external_url` stays (it keeps
  `gpui-pdf` independent of the editor).
- `vendor/gpui-pre-wgpu` is out of scope (its patch is governed separately).
- Unused dependencies found by the one-off scan are removed in the commit that
  orphans them.
- ADR 0030's visible change gets a CHANGELOG entry and README wording check.
- Superseded design docs get a one-line header pointing at the superseding ADR.

## Planned order

1. Hygiene: `.gitignore` Zorite entries, dangling `TODO.md` and
   `maintainability-review.md` references.
2. `gpui-pdf`: unconditional features, delete examples.
3. Delete `mdoc-markdown`'s `view`; move `syntax` into `mdoc-editor`; remove
   the crate.
4. Delete unused editor host hooks (spell-check hooks and demo kept, demo
   trimmed).
5. ADR 0030: stop rendering Zorite syntax; CHANGELOG.
6. Crate packaging: delete `API.md`, trim READMEs, `publish = false`.
7. Unused-dependency scan.
8. **Checkpoint** — test-removal list (criteria ii–iv) and any cohesion-based
   split proposals written here for approval.
9. Approved test changes.
10. Renames, CONTEXT.md and doc references.
