# Development

Folds: 0010, 0021, 0029, 0049, 0050, 0051, 0052, 0053, 0059, 0070.

## Build from source

All dependencies, including the pinned AnyDoc converter, are public.

```sh
git clone https://github.com/lawkitt/mdoc.git
cd mdoc
cargo run
# Or open a document:
cargo run -- path/to/document.pdf
```

Development commands are [just](https://just.systems) recipes, and CI runs
the same ones. `just setup` installs the pinned tools and `just doctor`
checks them; `just check` runs fmt, cargo-deny, clippy and the tests as CI
does, and `just check-all` adds the OCR and model smoke tests (downloads).

**The repository gate.** Before a PR is ready, run `just lint` and the tests
of what it changes: `cargo nextest run -p <crate>`, the app's with
`-p mdoc --features headless-renders`, a script's suite with
`bash scripts/<area>/test-<suite>.sh`, and `just test` for a change to
`Cargo.lock`, the toolchain or `.config/nextest.toml`. How the PR then
merges is in [specs/ci.md](specs/ci.md).

## Docs

Read this file and your area's spec, its source of truth.

- [specs/desktop.md](specs/desktop.md), [specs/mobile.md](specs/mobile.md),
  [specs/recognition.md](specs/recognition.md),
  [specs/cases-and-privacy.md](specs/cases-and-privacy.md): product rules.
- [specs/ci.md](specs/ci.md): how a PR reaches `main`.
- [specs/release.md](specs/release.md): releases, signing and acceptance.
- [ROADMAP.md](../ROADMAP.md) for open work, [Issues](https://github.com/lawkitt/mdoc/issues)
  for bugs, [CONTEXT.md](../CONTEXT.md) for terms,
  [privacy.md](privacy.md) for users.
- [adr/](adr/): the few architecture records kept, and where each
  folded number went.

### Writing docs

- A doc stays only if an agent would get something wrong without it and
  code, tests or lint don't already say so. History lives in git and PRs.
- Specs have one rule per bullet, with at most one "because" clause, no ADR
  numbers, paths only where an agent must go, and no narration. Each opens
  with a `Folds: NNNN, …` line.
- An ADR records only a hard-to-reverse choice: one that needs a migration
  or a rewrite to change, or picks a dependency or platform. Sections:
  Problem, Decision, Consequences; status `Status: Accepted, YYYY-MM-DD.`;
  no amendment chain. Product behaviour gets no new ADR.
- Never renumber an ADR: code cites them by number, and `docs/adr/README.md`
  resolves folded ones. New code doesn't cite ADRs.
- No design records in the tree: a grill's output is the spec change and its
  PR, with the record in the PR description or an issue.
- Documentation of one script or module lives beside it, in its header or a
  folder README. Agent guidance lives here: no `AGENTS.md` or `CLAUDE.md`.
- Don't commit survey or before-and-after PNGs; they go in the PR. Evidence
  stays only while a rerun script reads it. Platform acceptance results go in
  the release's issue.
- A PR that folds or trims rules lists each one it dropped, with a reason.
- `just lint` runs `scripts/dev/docs-lint.sh --check`: word budgets, no
  design or dated evidence folders, and every ADR number a file or folded.
  After an ADR change, run it bare to rewrite the index.

### Roadmap upkeep

- Bugs are GitHub issues, grouped by milestone and closed by `fixes #n`.
  `ROADMAP.md` holds the plan: milestone scope, priority order, Done-when
  lines, Later, Deferred and Not planned. Features need no issue.
- A bug enters the patch milestone when it gives wrong output, loses data or
  makes an action fail silently, and needs no design decision.
- Between groomings, a new bug becomes an issue and a new idea goes to the
  end of Later; don't reorder.
- Item shape: title, `[Theme]` tag, a one-line source, 1–4 lines of what and
  why, `*Done when:*`; about 6 lines. Diagnosis goes in the issue, linked.
- Only the patch lane and the next minor carry full items; Later is one line
  each, written out when promoted.
- Should-fix items never hold a release; a minor is scope-boxed at about
  five must-fix items.
- Finishing an item: the same commit deletes it from `ROADMAP.md` and adds
  its CHANGELOG entry as `changes/<slug>.md` (`changes/ios/` or
  `changes/android/` for an app change; format in
  [changes/README.md](../changes/README.md)).
- Groom after each release and acceptance run: close the finished milestone
  into Current status, triage issues, pick the next minor's must-fix items,
  promote from Later.

## Code layout

[ADR 0069](adr/0069-app-architecture-and-code-ownership.md) sets the layout
and ownership rules, and the table follows each move. Conversion and OCR
algorithm changes belong in the dependency forks.

**Where code goes:**

- New state goes into an owner with private fields, never onto `Workspace`
  or `DocumentView` unless it is view state (focus handles, scroll, hover).
- A state machine is a plain struct whose transitions are synchronous,
  GPUI-free and return that owner's effect enum; the view carries effects
  out. No trait hierarchy or service container.
- Logic that needs no GPUI goes into a GUI-free crate (`mdoc-store`,
  `mdoc-import`, `mdoc-detect`, `mdoc-pii`, …), with typed errors.
- A document reaches the window only through `TabEvent`. No glob imports
  outside test modules (except `gpui::prelude::*`), no `#[path]`; actions
  live with the feature that handles them. `just lint` checks boundaries;
  easement was declined ([#895](https://github.com/lawkitt/mdoc/issues/895)).
- A user-facing key binding is a row in `src/shortcuts.rs`; plumbing keys
  (Tab in a control group, moving in a menu, caret basics) stay with the
  feature.
- mdoc is an application: code the binary cannot reach is dead unless a
  named consumer exists. Crates are internal (`publish = false`, short
  READMEs).
- `unsafe` lives only in modules marked `#[allow(unsafe_code)]`, each block
  under a `// SAFETY:` comment. Miri, sanitizers and Valgrind were declined
  ([#897](https://github.com/lawkitt/mdoc/issues/897)).
- A refactor preserves behaviour; anything a user can observe needs its own
  decision. Judge it by clearer ownership, explicit effects and measured
  cost; file size alone never justifies a split.
- A test goes only when it exercises removed code, duplicates another test,
  asserts implementation detail or cannot fail, or when a lower-level test
  asserts the same thing. A UI test covers only what the real UI shows.
- UI tests boot through the real tab shell with production dependencies;
  fixtures supply deterministic settings, models and results, never real
  downloads.
- A closed tab or stale completion never changes another tab or frees a
  running worker's slot.
- Mobile app code follows [specs/mobile.md](specs/mobile.md).

| Location | Responsibility |
| --- | --- |
| `src/main.rs` | Startup, menus, key binding order (`bind_all_keys`), window creation |
| `src/shortcuts.rs`, `src/workspace/shortcuts_ui.rs` | The one table of user-facing key bindings, and the Keyboard Shortcuts sheet |
| `src/workspace.rs`, `src/workspace/` | The window shell: sidebar, tab identity, admission, scheduling, session persistence |
| `src/workspace/{tab_set,sidebar,focus_regions,notices,session_writer,docx_queue,quit_flow,case_coordinator,user_lists}.rs` | The window's owners: tabs, the sidebar, F6 regions, notices, session saves, the DOCX queue, close and quit, cases, the user-wide lists |
| `src/workspace/tab_menu.rs` | The sidebar tab menu's file and close rows |
| `src/workspace/{folder_sheet,folder_sheet_ui,case_page,shelf_source}.rs` | The folder sheet, the case page, and what the window gives the shelf |
| `src/settings_ui.rs`, `src/settings_ui/` | Settings categories, model actions, the dictionary, Detection's Always and Never lists |
| `src/document_view.rs`, `src/document_view/` | One tab: editor, source preview, import and OCR state, toolbar |
| `src/document_view/{case_link,document_lifecycle,import_flow}.rs` | The tab's owners: its case link, its lifecycle, its conversion |
| `src/pii.rs`, `src/pii/ui*` | Pseudonymization UI: scan and edit controller, Replacements panel, word popup |
| `src/{preview,comment_panel,markdown_search,search_session,images,style,models,spelling,updater,system_quit}.rs` | Preview, DOCX comments, find, local images, styling, model listings, spellcheck UI, the update check, macOS quit hooks |
| `src/shelf.rs`, `src/shelf/`, `src/menu_bar.rs`, `src/shelf_ui.rs`, `src/shelf_ui/` | The menu bar shelf's GPUI-free model, the menu bar item and the shelf's view |
| `src/document_view/tests.rs`, `src/workspace/tests.rs`, their `tests/`, `src/perf_tests.rs` | Headless flows, one file per owner, and opt-in performance tests |
| `src/workspace/headless_renders*`, `tests/headless_renders.rs`, `tests/headless_screens.txt` | Dev-only offscreen screen renders (the module header documents them) |
| `crates/mdoc-store` | Storage: app paths, atomic saves, sessions, recents, settings, encrypted cases and lists, crash-report masking |
| `crates/mdoc-import` | Import, offline OCR, resumable OCR for the apps, model downloads, the DOCX preview worker, recognition languages |
| `crates/mdoc-detect` | Offline detection: the GLiNER2 model stage, the correction stage, the user's lists, recognition fixtures and scoring |
| `crates/mdoc-pii`, `crates/mdoc-history` | The pseudonymization review model and its edit transactions |
| `crates/mdoc-editor`, `crates/mdoc-editor-core`, `crates/mdoc-markdown` | The GPUI editor; GUI-free editing rules and Markdown recognition shared with the apps |
| `crates/mdoc-spell` | The EN and RU spelling engine with bundled dictionaries |
| `crates/gpui-pdf`, `crates/gpui-bidi` | The PDF preview and bidirectional text layout |
| `crates/mdoc-ffi`, `apps/ios`, `apps/android` | The mobile core bindings and the apps ([iOS](../apps/ios/README.md), [Android](../apps/android/README.md)) |

**Dependencies.** `Cargo.toml` and `Cargo.lock` define dependency
revisions, `crates/mdoc-import/src/ocr.rs` the runtime URLs and digests, and
the pinned pdf-inspector manifest the OCR models; don't copy these
elsewhere. `Cargo.lock` must be what cargo resolves from `Cargo.toml`
(`just lint` fails otherwise): change a requirement in `Cargo.toml`, never
the lockfile alone. An exact pin carries its reason in a comment. Bump
`dirs`, `aes-gcm` and `docxide-pdf` deliberately, one PR each, checking
their paths, old cases and the preview corpus; `docxide-pdf` only when a
rendering fix needs it. Fork updates follow
[ADR 0001](adr/0001-conversion-and-ocr-in-dependency-forks.md). If
Cargo's Git fetch cannot authenticate, use
`CARGO_NET_GIT_FETCH_WITH_CLI=true`; never change global Cargo
configuration.

**Fixtures.** The [import](../tests/fixtures/import/README.md),
[DOCX](../tests/fixtures/docx-preview/README.md) and
[OCR](../tests/fixtures/ocr-qualification/README.md) notes give reproduction
and qualification limits. Never distribute local user-provided fixtures.

**Native checks.** Native appearance, shortcuts, IME, accessibility, GPU
presentation and installers need a native run; headless tests do not
establish them. `just perf` measures hotspots (its script's header has the
rules).

**Tests and OS services.** A test never asks the OS's security or identity
services: the code signature (`SecCodeCopySelf`), the case key (Keychain,
Windows data protection, Secret Service) or the build identity built on
them. Tests inject a case store (`Store::with_key`) and take the build label
as `Dev`. `mdoc-store`'s `deny-os-services` feature, on in the app's
dev-dependencies, makes those calls panic in every test build, and
`tests/os_services_guard.rs` proves it. A new call into such a service goes
behind the same guard.

## Recipes

`just` lists every recipe; Git Bash runs them on Windows.

| Recipe | What it does |
| --- | --- |
| `just setup` / `just doctor` | Install / check the pinned toolchain and tools |
| `just check`, `just check-all` | The CI gate; plus the smoke recipes |
| `just test`, `just test-tier <tier>` | Every test; one tier (`unit`, `app`, `renders`, `recognition`, `rust`, `scripts`, `quarantined`) |
| `just lint-scripts`, `just actionlint` | shellcheck and shfmt over `scripts/`; workflow lint |
| `just smoke-ocr`, `just smoke-models`, `just smoke-package`, `just smoke-update` | Release gates |
| `just perf` | Ignored performance tests, serially |
| `just recognize`, `just import`, `just detection-gate`, `just snapshots-fetch` | Recognition fixtures ([guide](../tests/fixtures/recognition/README.md)) |
| `just headless-renders <new-dir>` | Offscreen light and dark PNGs, Tab walks and frame costs |
| `just package`, `just release`, `just release-ios` | Installers into `dist/`; releases ([specs/release.md](specs/release.md)) |
| `just ios-core`, `just ios-app`, `just ios-test`, `just ios-release`, `just ios-device-test`, `just mobile` | The iOS core, app, tests and device runs |
| `just android-setup`, `just android-emulator`, `just android-core`, `just android`, `just android-device-test` | The Android SDK, emulator, core, CI job and device runs |
| `just ios-survey <out>`, `just android-survey <out>` | Every screen, light and dark, into a folder outside the tree (such as `target/ios/survey`); not CI |
| `just icon`, `just disk`, `just retire-worktree <path>` | Icon assets from `build/appicon.svg`; worktree `target/` sizes; remove a finished worktree |

A slow test is a bug to fix, not a limit to raise; tests never retry.

### Shell scripts

Recipes stay one command; anything longer is a Bash script in
[scripts/](../scripts/README.md), no `xtask`. Each starts with
`#!/usr/bin/env bash` and a `# Usage:` header, then sources `lib.sh`, which
sets `-euo pipefail`, moves to the repository root and provides `die`,
`platform`, `confirm` and `load_tools`. Scripts run in Git Bash on Windows;
one whose header says so stays Bash 3.2 compatible for macOS's `/bin/bash`.
`shfmt -w scripts` applies the style in `.editorconfig`.

## Build cache

- The dev and test profiles keep line tables for workspace crates and no
  debug information for dependencies. For a debugger, build into
  `target/debugging/`: `cargo run --profile debugging`.
- A dependency gets its own dev `opt-level` (2) only when a profile shows it
  hot and the cold test build grows no more than about 30% on both macOS and
  Windows; blanket levels stay off.
- Don't override profiles or `RUSTFLAGS` through environment variables, and
  never share a `CARGO_TARGET_DIR` between worktrees: both invalidate
  artifacts, and a shared one tested stale code.
- Each worktree has its own `target/`, with no cap or automatic eviction.
  `just disk` reports their sizes;
  `just retire-worktree <path>` deletes a merged worktree's `target/` and
  then the worktree, refusing uncommitted, untracked or ignored files.
- Cargo never deletes superseded artifacts: after a toolchain bump or a
  large dependency update, run `cargo clean --profile dev`.

## Fleet

- Other machines: the maintainer's `fleet` skill.
- `wsl` builds only the platform-neutral crates. A lane stops and reports
  before adding a dependency, vendoring code, patching `vendor/` or a GPUI
  platform crate, or editing outside its item.
- A dev build uses the real profile: copy `~/Library/Application Support/mdoc`
  aside before a GUI check and restore it after.

**Profile.** The fleet tools in the maintainer's config repository read
this block; change it with the rules above.

```toml fleet-profile
version = 1
base = "main"
roadmap = "ROADMAP.md"
read_first = [
  "`docs/development.md` (Fleet, Code layout, repository gate)",
  "`CONTEXT.md` (the terms the item uses)",
]
# A fast subset (Build from source): the app is `-p mdoc --features
# headless-renders`, a script's suite `bash scripts/<area>/test-<suite>.sh`, and a
# change to Cargo.lock, the toolchain or .config/nextest.toml runs `just test`.
gate = ["just lint", "cargo nextest run -p <each changed crate>"]
merge_queue = true
checks = ["lint (ubuntu)", "merge-gate"]
rules = [
  "Never run the app against the real profile (`~/Library/Application Support/mdoc`, `%LOCALAPPDATA%\\mdoc`) or touch the real Keychain key; tests use injected stores.",
  "Stop and report to the hub before adding a dependency, vendoring code, patching `vendor/` or a GPUI platform crate, or editing outside the item's area.",
]

[disk]
per_worktree_gib = 30
first_build_gib = 40
warm_gib = 15

[renders]
produce = "just headless-renders <new-directory>"
fetch = "dispatch `ci-desktop.yml` on the branch with `-f os=macos -f screens=<globs>` and `gh run download <run> --name renders-macos-screens`"
done_when = "headless renders of {what}, light and dark (the hosted desktop job uploads them)"

[bookkeeping]
roadmap_item = "remove item {number} from `{roadmap}` {milestone} (delete only its own lines; do not renumber)"
issue = "the commit that finishes it says `fixes #{number}`"
fragment = "add `changes/{slug}.md`, or `changes/ios/{slug}.md` / `changes/android/{slug}.md` for an app change (format in `changes/README.md`)"

[machines.macbook-air]
setup = "Never set `CARGO_TARGET_DIR`: this worktree builds into its own `target/` (a shared one gives stale builds). First build is cold."

[machines.windows]
disk_check = "`Get-PSDrive E` (not `just disk`, which fails while another lane builds)"
lanes = { windows-lane-1 = 'E:\t3\worktrees\mdoc\windows-lane-1', windows-lane-2 = 'E:\t3\worktrees\mdoc\windows-lane-2', windows-lane-3 = 'E:\t3\worktrees\mdoc\windows-lane-3', windows-lane-4 = 'E:\t3\worktrees\mdoc\windows-lane-4' }
notes = "Every lane, the root checkout and the queue runners' homes (`E:\\ci`) are on `E:`, a ReFS Dev Drive (a VHDX on `D:`); `CARGO_HOME` and `RUSTUP_HOME` stay on `C:`."

[machines.wsl]
disk_check = "`df -h ~`"
lanes = { wsl-lane-1 = "~/.t3/worktrees/mdoc/wsl-lane-1" }
gate = ["cargo test -p mdoc-pii -p mdoc-editor -p mdoc-history"]
notes = "Only the platform-neutral crates build here; a Linux build of the app is not a shipped target."
```
