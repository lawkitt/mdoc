# Development notes

## Build from source

All dependencies, including the pinned AnyDoc converter, are public.

```sh
git clone https://github.com/lawkitt/mdoc.git
cd mdoc
cargo run
# Or open a document:
cargo run -- path/to/document.pdf
```

Development commands are [just](https://just.systems) recipes; CI runs the
same ones. `just setup` installs the pinned tools, `just doctor` checks them.

```sh
just check      # fmt, cargo-deny, clippy and tests, as CI runs them
just check-all  # plus the OCR and installed-model smoke tests (downloads)
```

The full list is under [Recipes](#recipes).

## Project documentation

- [Roadmap](../ROADMAP.md): open work by milestone; bugs are [Issues](https://github.com/lawkitt/mdoc/issues).
- [Glossary](../CONTEXT.md): product and code terminology.
- [Architecture decisions](adr/): accepted behavior and its rationale.
- [Evidence](evidence/): measurements that current work still compares against.
- [Privacy and limitations](privacy.md).
- [Release runs](release-run.md): standing rules for cutting an RC and running
  its platform acceptance in parallel on the fleet.

### Writing ADRs

ADRs state the rules in force ([ADR 0052](adr/0052-adrs-are-current-rules.md));
history lives in git.

- Sections: Problem, Decision, Invariants (if any), Consequences, then an
  optional `References:` line for interview records and evidence.
- Status line: `Status: Accepted, YYYY-MM-DD. Amended by ADR X; superseded in
  part by ADR Y.` No conversation narration.
- A later decision edits the older ADR to show only what still holds, with a
  one-line pointer. An ADR with no surviving rule becomes a stub: title,
  `Superseded by ADR NNNN`, and a commit permalink to its full text.
- Never renumber: code cites ADRs by number.
- Design interviews and dated evidence leave the tree once nothing live reads
  them; link them by commit permalink.

### Roadmap upkeep

[ADR 0059](adr/0059-roadmap-after-0-1-0.md) sets the rules.

- **New finding:** a bug becomes a GitHub issue (with the patch milestone if it
  meets the patch rule); an idea goes to the end of Later. Don't reorder.
- **Finishing an item:** the same commit deletes it from ROADMAP.md and adds
  its CHANGELOG entry as `changes/<slug>.md`
  ([ADR 0072](adr/0072-changelog-fragments.md), format in
  [changes/README.md](../changes/README.md)); a patch commit says `fixes #n`.
- **Item shape:** title, `[Theme]` tag, one-line source, 1–4 lines of what and
  why, `*Done when:*`; about 6 lines. Diagnosis goes to the issue, evidence
  or ADR, linked.
- **Grooming** after each release and acceptance run: close the finished
  milestone into Current status, triage open issues, pick at most ~5 must-fix
  items for the next minor, promote from Later, and update the date line.

## Code layout

Run the [repository gate](#build-from-source) before delivering code changes.

Keep build caches for fast warm rebuilds. For occasional cleanup from the
repository root, use `cargo clean --workspace` to remove workspace artifacts
while retaining dependency artifacts. Use `cargo clean` for a full reset;
the next build will be cold. Cleanup is manual; no automatic size limit applies.
See [Cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html).

[ADR 0069](adr/0069-app-architecture-and-code-ownership.md) sets the layout
and ownership rules; the refactoring it phases is moving code, and this table
follows each move. Conversion and OCR algorithm changes belong in the
dependency forks.

**Where code goes:**

- New state goes into an owner with private fields, never onto `Workspace` or
  `DocumentView` unless it is view state (focus handles, scroll, hover).
- A state machine is a plain struct whose transitions are synchronous,
  GPUI-free and return that owner's effect enum; the view carries effects out.
- Logic that needs no GPUI goes into a GUI-free crate (`mdoc-store`,
  `mdoc-import`, `mdoc-detect`, `mdoc-pii`, …), with typed errors.
- A document reaches the window only through `TabEvent`; the window calls
  document methods. No glob imports outside test modules (except
  `gpui::prelude::*`), no `#[path]`; actions live with the feature that
  handles them. A user-facing key binding is a row in `src/shortcuts.rs`
  naming the `bind_keys` that registers it; plumbing keys (Tab inside a
  control group, moving inside a menu or list, caret basics) stay with the
  feature ([ADR 0073](adr/0073-keyboard-shortcuts-panel.md)).

| Location | Responsibility |
| --- | --- |
| `src/main.rs` | Application startup, menus, key binding order (`bind_all_keys`) and window creation |
| `src/shortcuts.rs`, `src/workspace/shortcuts_ui.rs` | The one table of user-facing key bindings, which every `bind_keys` and the palette read, and the Keyboard Shortcuts sheet that lists it ([ADR 0073](adr/0073-keyboard-shortcuts-panel.md)) |
| `src/workspace.rs`, `src/workspace/` | The window shell: document sidebar, tab identity, admission, scheduling and session persistence |
| `src/workspace/{tab_set,sidebar,focus_regions,notices,session_writer,docx_queue,quit_flow,case_coordinator}.rs` | The window's owners: tabs and the active tab, the sidebar and its reveal, F6 regions, the bell's notices, session saves, the DOCX worker queue, closing and quitting, and case coordination |
| `src/settings_ui.rs`, `src/settings_ui/` | Settings categories, save-on-change rows, model actions and the Spelling dictionary drill-in ([ADR 0076](adr/0076-settings-categories.md)) |
| `src/workspace/folder_sheet.rs`, `src/workspace/folder_sheet_ui.rs` | The folder sheet: a dropped folder's files, ticks and 20-file cap, and the queue of folders waiting their turn; the dialog and its case actions ([ADR 0075](adr/0075-folder-sheet.md)) |
| `src/workspace/case_page.rs` | The transient case page: cached overview, documents, names, colour, detection categories and case actions ([ADR 0077](adr/0077-case-page.md)) |
| `src/document_view.rs`, `src/document_view/` | One tab's document view: editor, source preview, import/OCR state, actions and toolbar chrome |
| `src/document_view/case_link.rs` | The tab's case-link owner: its case, the case's identities and skipped categories, and the pending save into the case |
| `src/document_view/document_lifecycle.rs` | The tab's lifecycle owner: loading, availability, source-only, unsaved changes, untouched conversions and placeholders, and its open dialog |
| `src/document_view/import_flow.rs` | The tab's conversion owner: the import job and slot, automatic conversion, OCR consent, recognition languages and setup |
| `crates/mdoc-store` | GUI-free storage: app paths, atomic document saves and document identity, the session manifest, recents, settings types and files, encrypted saved cases, crash-report masking |
| `crates/mdoc-import` | GUI-free import: library integration and pending jobs (`import`, `import_session`), explicit verified setup and offline OCR (`ocr`), model downloads and the one-at-a-time model permit, and the supervised DOCX preview worker (`docx_preview`; its entry point stays in `src/main.rs`) |
| `src/models.rs` | What setup and Settings show for each downloadable model, from the engine that owns it |
| `crates/mdoc-import/src/recognition.rs` | Recognition languages: a document's default list, its reason and the keyboard input-source signal ([ADR 0066](adr/0066-recognition-language-defaults.md)) |
| `src/preview.rs` | Preview ownership |
| `crates/mdoc-import/src/docx_preview/comments.rs`, `src/comment_panel.rs` | Read-only DOCX comments |
| `src/markdown_search.rs`, `src/search_session.rs` | Markdown find controls and revision-aware search scheduling |
| `src/pii.rs`, `src/pii/ui*` | Pseudonymization UI: scan/edit controller, Replacements panel and word popup, UI regression tests |
| `crates/mdoc-detect` | GUI-free offline detection: the GLiNER2 detector, its verified setup and the structured rules |
| `src/images.rs`, `src/style.rs` | Document-relative local images and app styling |
| `src/document_view/tests.rs`, `src/workspace/tests.rs` and their `tests/` folders, `src/perf_tests.rs` | Headless flows, one file per owner beside shared helpers, ownership/lifetime checks, and opt-in performance measurements |
| `src/workspace/headless_renders.rs`, `tests/headless_renders.rs` | Dev-only offscreen screen/Tab evidence and the output contract test |
| `crates/mdoc-editor` | Host-agnostic WYSIWYG, rendered-text search, and Markdown recognition |
| `crates/mdoc-pii`, `crates/mdoc-history` | GUI-free pseudonymization review model and the plain edit transactions it follows |
| `crates/gpui-pdf`, `crates/gpui-bidi` | Virtualized PDF preview and bidirectional text layout |
| `src/updater.rs` | Detection-only GitHub Releases update check ([ADR 0038](adr/0038-update-check.md)) |
| `src/spelling.rs`, `crates/mdoc-spell` | Spellcheck: background checks, toolbar indicator and navigation; GUI-free EN/RU engine with bundled dictionaries ([ADR 0036](adr/0036-bundled-spellcheck.md)) |

Cargo.toml and Cargo.lock define dependency revisions; `crates/mdoc-import/src/ocr.rs` defines
runtime URLs/digests, and the pinned pdf-inspector manifest defines OCR model
artifacts. Avoid copying these into another configuration. Cargo.lock must be
what cargo resolves from Cargo.toml; `just lint` fails otherwise. Change a
dependency's requirement in Cargo.toml, never the lockfile alone
([ADR 0053](adr/0053-cargo-lock-matches-manifest.md)). AnyDoc supplies
non-PDF conversion; pdf-inspector handles PDFs directly.

For fork updates, test the fork first, review lockfile/transitive changes, then
run the [repository gate](#build-from-source) and compare outputs against
the source before changing fixture expectations. AnyDoc's PR 153 port is attributed in its fork; the fork
began as a snapshot with unrelated upstream history, so port reviewed upstream
changes rather than assuming a normal merge. Its transitive pdf-inspector 1.14.2
baseline is distinct from the app's direct OCR-capable fork. If Cargo's Git fetch
cannot authenticate, use `CARGO_NET_GIT_FETCH_WITH_CLI=true` with existing Git
credentials; do not change global Cargo configuration.

Use [import](../tests/fixtures/import/README.md),
[DOCX](../tests/fixtures/docx-preview/README.md), and
[OCR](../tests/fixtures/ocr-qualification/README.md) fixture notes for reproduction
and qualification limits. Native appearance, shortcuts, IME, GPU presentation,
and execution on other platforms require native checks; headless tests do not
establish them. Do not distribute local user-provided fixtures.

Measure runtime hotspots separately from the routine gate with `just perf`,
which runs every ignored performance test serially (set `MDOC_PERF_PREVIEW` to
a local PDF or DOCX to include preview timing). Run it on an idle machine.
`MDOC_PERF_PROFILE=release just perf` uses the release profile, serial builds
and unchanged ceilings. The runner attempts every check before returning failure.
The host matrix includes the ordinary legal-document workloads: a synthetic
~30-page agreement (`contract`) and a scrubbed real 37-page legal opinion
(`legal_opinion`, from `tests/fixtures/docx-preview/legal-opinion.docx`);
`MDOC_PERF_WORKLOADS=contract,tables` limits the matrix to named workloads when
profiling one of them.
The DOCX scroll workload runs the public `coverage.docx` and `legal-opinion.docx`;
`MD_PERF_FILE` selects one DOCX under `tests/fixtures/docx-preview` instead.
Scrub a private DOCX into a distributable layout fixture with
`tests/fixtures/docx-preview/scrub.py`; never commit the original.
Each host-matrix workload also steps through a common word (`the`); headless
text shaping is not Core Text, so confirm shaping-heavy changes natively with
the [native check](evidence/editor-layout/2026-10-17-native/README.md) tools. The PDF check defaults
to public `reference.pdf` plus a generated 64-page PDF; `PDF_PERF_DIR` selects
an explicit local corpus.
Logs include actual editor widths; outer window dimensions alone are insufficient
for comparisons.
`review_host_performance` covers diagnostic density, proposed/applied review,
before/after views and three-member case synchronization without a detector download.
Its `plain_layout` phase draws the same text without review state at the same
editor width, so each review phase's excess over it is the PII overhead.
`MDOC_PERF_STRESS=1` makes that review matrix use 2 MiB / 20,000 mentions;
run it explicitly in release mode on an idle host for stress measurements.

Use a fixed viewport, representative legal/OCR Markdown, and actual scroll/search
positions; report profile, machine, latency, peak memory, and lifecycle results.
Historical tab measurements found roughly 292 ms long-document redraws in
debug/headless mode despite a 0.007 ms tab activation handler. Reproduce before
optimizing; those timings are not current results or native presentation latency.

## Recipes

Every development command is a `just` recipe, run by Git Bash on Windows
(install [Git for Windows](https://gitforwindows.org) and `just`). CI calls the
same recipes, and the tool versions in `scripts/tools.env` are shared by CI and
`just setup` ([ADR 0037](adr/0037-ci-and-release-pipeline.md)).

| Recipe | What it does |
| --- | --- |
| `just setup` / `just doctor` | Install / check the pinned toolchain and tools |
| `just check` | `fmt-check`, `deny`, `lint-scripts`, `lint`, `test` — the CI gate |
| `just lint-scripts` | `shellcheck` and `shfmt -d` over `scripts/` |
| `just smoke-ocr` | Downloads the pinned OCR runtime and recognizes the EN/RU fixtures (release gate) |
| `just smoke-models [--download]` | Default OCR and pseudonymization models through setup and offline paths |
| `just check-all` | `check` + both smoke recipes |
| `just perf` | Ignored performance tests, serially |
| `just headless-renders <new-directory>` | macOS/Windows: light/dark PNGs, actual Tab walks and frame costs, entirely offscreen |
| `just package` | This machine's installers into `dist/` |
| `just smoke-package` | OCR from the packaged artifact: the `.dmg`'s signed app, or the release `mdoc.exe` (release gate) |
| `just smoke-update [dmg]` | An older Developer ID signed `.dmg` updates itself to the latest release through `--mdoc-self-update` (manual, before releases that change the updater) |
| `just release <version>` | Release commit and tag (below) |
| `just icon`, `just actionlint` | Icon assets; workflow lint |
| `just disk` | Size of each worktree's `target/` |
| `just retire-worktree <path>` | Remove a finished worktree and its `target/` |

Heavy, download and machine-dependent tests run only through these recipes, on
the Apple Silicon Mac and the Windows x64 PC.

### Shell scripts

Recipes stay one command; anything longer is a Bash script in `scripts/`
([ADR 0070](adr/0070-ci-speed-and-script-hygiene.md)). Each starts with
`#!/usr/bin/env bash` and a `# Usage:` comment, then sources `scripts/lib.sh`,
which sets `-euo pipefail`, moves to the repository root and provides `die`,
`platform`, `confirm` and `load_tools`. Scripts run in Git Bash on Windows;
a script that says so in its header (`release-ci.sh`) stays Bash 3.2
compatible for macOS's `/bin/bash`. `just lint-scripts` runs
`shellcheck` and `shfmt`, whose style (two-space indent, indented `case`
branches) lives in `.editorconfig`; `shfmt -w scripts` applies it.

<a id="headless-screen-renders-macos"></a>

### Headless screen renders (macOS and Windows)

```sh
just headless-renders ./mdoc-renders
```

The output directory must be new and its parent must exist. The recipe builds
the dev binary with `headless-renders`; the runner is absent in release builds,
even with that feature enabled. It uses GPUI's `HeadlessAppContext`, native
text shaping (Core Text on macOS, DirectWrite on Windows). macOS uses the
headless Metal texture renderer; Windows uses a test-only DirectX 11 WARP
software device and offscreen textures, with staging readback to RGBA. The
Windows adapter constructs neither `WindowsPlatform` nor an HWND, swap chain
or DirectComposition target. All windows are `TestWindow`s: no native window,
application event loop, activation or screen capture. It can run while someone else uses the desktop.

Every run creates and removes a temporary profile, redirects app data and cache
paths before building app state, injects a temporary case store and disables OS
case-key access. It reads only public fixtures, never the real mdoc profile,
clipboard or installed models; it downloads nothing. The gate's integration
test on macOS runs the child under `sandbox-exec`, denying the real
profile/update cache and network access. `just lint` and `just test` enable
this dev feature; on both platforms the latter checks the actual generated
files and profile cleanup. On Windows, PNGs are encoded on a bounded
background thread, which must finish without error before the summary is
written. The `test (windows-latest)` CI job runs this test through
`just test`. On Windows,
the integration test also polls all top-level HWNDs owned by the child
(including hidden ones) and fails if it creates any; the check filters by
child process ID and leaves other apps untouched.

CI sets `MDOC_HEADLESS_ARTIFACTS` to a new directory under the runner's temporary
folder and uploads that same contract-tested output, with one artifact per
platform and run attempt. Local `just test` keeps its output temporary unless
this variable names a new output directory whose parent already exists.

Output layout:

```text
mdoc-renders/
  summary.json
  light/
    home.png
    home/tab-000.png ...
    document.png
    document/tab-000.png ...
    review.png ...
  dark/
    home.png ...
```

The output also includes paired `focus-toggle-on`, `focus-sidebar-row` and
`focus-settings-row` PNGs in both themes, each with a `-rest` counterpart.
The Settings pair targets the first Detection model radio, reached by one Tab
from the selected category tab.
`focus-beside-selected-tab` also checks a focused Review tab beside the
selected Share tab.
`focus-comments-toggle` uses the public DOCX comments fixture to check the
ring at the Original pane's clipped edge.
`focus-decision-row` checks a row of What you did beside done and undone entries.
`summary.json` lists these under `focus_rings`. The contract test compares
pixels after focus scrolling settles: only the 2 px accent outline changes,
preserving the control's fill and label and checking that both sides of the
ring remain visible.

Both themes cover Home, a document and its ⋯ menu, Review, the mention popup, model setup
consent, the OCR language card, all six Settings categories (General, Detection, Recognition,
Spelling, Cases and About), the dictionary, command palette, the keyboard shortcuts
sheet, Find and Replace,
before/after, What you did with done and undone entries, Open Case, the unlocked and locked case page, case rename/categories, the Keychain explainer preview,
the folder sheet (a small folder, one over the 20-file cap, and Add to a
case), confirmations for case deletion, mapping retention, unsaved changes and model
removal, model comparison, a damaged DOCX's failed conversion and preview,
and a restored AI reply. The legal review uses
`pseudonymization/corpus.json` annotations as deterministic proposals; Restore
uses the captured `restore/claude-en` fixture. No detector runs. Platform file
pickers and the OS Keychain password prompt are outside this app-owned set.
Each Settings baseline records and checks the selected category; the dictionary
opens in Spelling. The General baseline shows Appearance with System, Light and
Dark visible; its state records both controls' theme choices and checks the
actual segments' bounds against the scroll viewport for each render.

PNGs are 2200 × 1520 (1100 × 760 points at 2×). `summary.json` schema version 1
contains a record per screen/theme: baseline PNG, ordered `tab_order` entries
with run-local `focus_id`, `key_contexts`, focus-ring PNG and input cost;
`cycle_to` indexes the first repeated focus and `truncated` flags the 128-press
limit. These are actual Tab keystrokes, so the editor can insert indentation
and a panel can keep Tab in its own loop. The PNG for each step identifies the
control; focus IDs are not stable labels across runs. A local loop is not a
claim that Tab reached every window control (F6 regions remain separate).

`frames` records each explicit settle/Tab frame's CPU `layout_paint_ms`;
`raster_readback_ms` separately includes synchronous GPU rendering and readback.
PNG encoding is excluded. Timings describe this dev run, without pass/fail
budgets; use an idle machine to compare them. These images do not establish
native input, IME, accessibility or display-presentation acceptance.

**Windows isolation limits:** app data/cache roots are redirected before app
state is constructed, and the runner checks they remain under its temporary
profile. Both OS case-key entry points refuse in this mode (no DPAPI access);
the runner exercises those guards. All app HTTP request entry points reject
requests in this mode, and the model-download guard is exercised before any
render. The GPUI test HTTP client has no network backend. This is application
isolation, not an OS security boundary: Windows does not run `sandbox-exec`,
and the test applies no filesystem ACL, AppContainer or firewall policy.
A new unguarded native filesystem/network call would not be denied by Windows.
No permissions or files in the real `%LOCALAPPDATA%\mdoc` are changed, and the
installed app can continue running in parallel. Native presentation and GPU
hardware behavior remain outside this software-rendered test.

## Build cache and disk use

The dev and test profiles keep line tables for workspace crates and no debug
information for dependencies ([ADR 0050](adr/0050-rust-build-cache-growth.md)).
Backtraces (`RUST_BACKTRACE=1`) still show file and line, but a debugger cannot
inspect variables. For lldb or CodeLLDB, build with full debug information in a
separate `target/debugging/`, leaving the everyday cache untouched:

```sh
cargo run --profile debugging
cargo test --profile debugging -p mdoc-editor --no-run
```

Keep one build configuration: don't override profiles or `RUSTFLAGS` through
environment variables, and don't share a `CARGO_TARGET_DIR` between worktrees.
Either one invalidates artifacts and leaves duplicate copies behind.

Each worktree has its own `target/`, and nothing removes it automatically.
`just disk` reports their sizes. When a worktree's branch is merged or
abandoned, `just retire-worktree <path>` deletes its `target/` directories and
then the worktree, keeping the branch. It refuses a worktree with uncommitted,
untracked or other ignored files (such as `dist/` or `.qualification/`), which
`git worktree remove` would otherwise delete silently.

Cargo never deletes superseded artifacts. A toolchain bump, a large dependency
update or a changed flag builds a complete new copy beside the old one, so run
`cargo clean --profile dev` after one (the next build is cold, about two
minutes for `just test`). Edits alone grow the cache only slowly.

## Fleet

Agents that dispatch work to the maintainer's other machines (the `fleet`
skill) read this section for mdoc's needs.

- **Where work goes.** Windows-only bugs and Windows checks: `windows`.
  `wsl` runs only the platform-neutral crates (`cargo test -p mdoc-pii -p
  mdoc-editor -p mdoc-history`); a Linux build of the app is not a shipped
  target. Model qualification on the `wsl` GPU needs a decision first: the app
  runs detection on CPU.
- **Lane worktrees.** A worktree's `target/` reaches about 30 GiB and a cold
  `just lint` + `just test` takes about 4.5 minutes on `windows`, so workers
  reuse lanes instead of a new worktree per task: `windows-lane-1` and
  `windows-lane-2` on `windows`, `wsl-lane-1` on `wsl`. Each task starts with
  `git switch -c <machine>/<topic> origin/main`.
  Retire other worktrees with `just retire-worktree <path>` once merged.
  A third lane (`windows-lane-3`) is added only when `df` shows room for
  another 30 GiB with margin. Claude Desktop SSH sessions use these lane
  folders too (`use_worktree: false`), one session or T3 thread per lane.
- **Models and reviews** follow the fleet skill (fleet ADRs 0016, 0017):
  Opus 5.5 builds, Windows-specific items included; Codex Sol builds only
  while Claude is limited. Every code PR gets the other provider's review
  right after it is marked ready, beside CI; when the reviewer is limited
  the review is skipped and the PR merges on the hub's gate. Lanes leave
  `ROADMAP.md` alone: the hub removes merged items in one docs-only commit.
- **Hub worktrees on `macbook-air`** keep their own `target/`, like every
  worktree: never point `CARGO_TARGET_DIR` at the root checkout's. Shared, it
  gave the workspace crates one build hash across worktrees, so a build reused
  another worktree's `mdoc-pii` and tested stale code (2026-10-29). Check `df`
  for about 30 GiB per worktree first, and retire it when merged. At most two
  worktrees build at once on the Mac.
- **Native checks.** The [acceptance](platform-acceptance.md) checklist runs
  by computer use (see [ADR 0057](adr/0057-native-platform-acceptance.md));
  rows that need a physical keyboard, speech, passwords or system settings
  stay with the maintainer.
- **Release runs** follow [release-run.md](release-run.md).

**Profile.** The fleet tools (`fleet-brief`, `fleet-review` in the
maintainer's config repository) read this block; it restates the rules
above for them, so change both together.

```toml fleet-profile
version = 1
base = "main"
roadmap = "ROADMAP.md"
read_first = [
  "`docs/development.md` (Fleet, Code layout, repository gate)",
  "`CONTEXT.md` (the terms the item uses)",
]
gate = ["just lint", "just test"]
rules = [
  "Never run the app against the real profile (`~/Library/Application Support/mdoc`, `%LOCALAPPDATA%\\mdoc`) or touch the real Keychain key; tests use injected stores.",
]

[disk]
per_worktree_gib = 30
first_build_gib = 40
warm_gib = 15

[renders]
produce = "just headless-renders <new-directory>"
fetch = "gh run download <run-id> --repo lawkitt/mdoc --pattern 'headless-renders-*'"
done_when = "headless renders of {what}, light and dark (CI uploads them)"

[bookkeeping]
roadmap_item = "remove item {number} from `{roadmap}` {milestone} (delete only its own lines; do not renumber)"
issue = "the commit that finishes it says `fixes #{number}`"
fragment = "add `changes/{slug}.md` (format in `changes/README.md`)"

[machines.macbook-air]
setup = "Never set `CARGO_TARGET_DIR`: this worktree builds into its own `target/` (a shared one gives stale builds). First build is cold."

[machines.windows]
disk_check = "`Get-PSDrive C` (not `just disk`, which fails while another lane builds)"
lanes = { windows-lane-1 = 'C:\Users\tebri\.t3\worktrees\mdoc\windows-lane-1', windows-lane-2 = 'C:\Users\tebri\.t3\worktrees\mdoc\windows-lane-2' }

[machines.wsl]
disk_check = "`df -h ~`"
lanes = { wsl-lane-1 = "~/.t3/worktrees/mdoc/wsl-lane-1" }
gate = ["cargo test -p mdoc-pii -p mdoc-editor -p mdoc-history"]
notes = "Only the platform-neutral crates build here; a Linux build of the app is not a shipped target."
```

## Releasing

Releases ship only for Apple Silicon macOS (`.dmg`) and Windows x64 (NSIS
`.exe`; `.msi` for stable versions). The repository version is the source of
truth: `just release 0.1.0-beta.1` checks that `main` is clean and current, runs
the checks (optionally `check-all`), sets the version in `Cargo.toml`,
`Cargo.lock` and `Info.plist`, moves the `changes/` fragments into the
CHANGELOG's Unreleased section, names that section for stable versions,
commits `release: v…`, tags, and pushes after confirmation. The tag runs
`.github/workflows/release.yml`: version check, CI with the OCR smoke test on
both targets, packaging and the packaged-app gate, then a draft GitHub Release
with `SHA256SUMS` and build provenance. Review the draft and publish it by
hand. "Run workflow" on the Release workflow builds an unpublished test build
from any pushed branch; it goes straight to packaging (the branch already ran
CI) unless **checks** is ticked, and signs the macOS app only with **sign**.

For a run without prompts, `just release <version> --yes` (`--check-all` opts
into the smoke tests; `--no-push` wins over the automatic push).
`--skip-checks-if-ci-green` skips the local checks only when the latest
CI-relevant commit on `origin/main` has a successful CI push run, or, for a
`release: v…` commit, a successful Release run on its tag. Anything uncertain
falls back to the local checks.

Both paths reuse the packaged NSIS artifact in a separate GitHub-hosted Windows
desktop job: silent install, fresh-profile launch, UI Automation main-window
check, screenshot, clean quit and uninstall. The `installed-windows-desktop`
artifacts (`-startup` and `-wrong-title`) contain screenshots, UIA evidence and
the log. An impossible-title case proves startup failures fail and keep evidence.
The job gates draft releases; it never installs on a fleet machine. This smoke
establishes startup only, not the manual platform acceptance checklist.

### macOS signing

Signed builds are Developer ID signed and notarized
([ADR 0039](adr/0039-macos-signing-and-notarization.md)) from the `release`
environment. To set it up, or to replace an expiring certificate, the account
holder first opens a shell in a directory whose direnv `.envrc` loads the
`CERTIFICATE_OSX_*` variables from the Keychain (`cd` there on its own, so
direnv runs before the next command), then runs the commands below. Values are
piped, never on a command line, and an empty variable stops the loop: an
empty secret would only fail later, at the signing step.

```sh
gh api -X PUT repos/lawkitt/mdoc/environments/release
for pair in APPLE_CERTIFICATE=CERTIFICATE_OSX_P12_DATA APPLE_CERTIFICATE_PASSWORD=CERTIFICATE_OSX_P12_PASSWORD APPLE_ID=CERTIFICATE_OSX_ID APPLE_PASSWORD=CERTIFICATE_OSX_APP_PASSWORD; do
  src=${pair#*=}
  [[ -n ${(P)src} ]] || { echo "$src is empty"; break; }   # zsh
  printf %s "${(P)src}" | gh secret set "${pair%%=*}" --env release -R lawkitt/mdoc
done
gh variable set APPLE_TEAM_ID --env release -R lawkitt/mdoc --body F33DLM8K53
gh variable set APPLE_SIGNING_IDENTITY --env release -R lawkitt/mdoc --body "Developer ID Application: Tebriz Tagiev (F33DLM8K53)"
```

Then run the Release workflow with **sign** ticked to try the signed path
without a tag. To sign locally, import the certificate into the login
keychain, store notarization credentials once with
`xcrun notarytool store-credentials mdoc-notary`, then run
`APPLE_SIGNING_IDENTITY="Developer ID Application: Tebriz Tagiev (F33DLM8K53)"
APPLE_KEYCHAIN_PROFILE=mdoc-notary just package`.

### Case key and Keychain access

With a Developer ID provisioning profile, the case key lives in the data
protection keychain, and macOS never asks for the login password
([ADR 0045](adr/0045-prompt-free-case-key.md)). Until the profile exists,
signed builds use the login keychain. To turn it on, the account holder
registers the App ID `io.github.lawkitt.mdoc` with **Keychain Sharing**,
creates a **Developer ID** provisioning profile for it, and stores it:

```sh
base64 -i mdoc.provisionprofile | gh secret set APPLE_PROVISIONING_PROFILE_BASE64 --env release -R lawkitt/mdoc
```

Locally, set `APPLE_PROVISIONING_PROFILE=<path>` next to the signing variables.
Unsigned, ad-hoc and debug builds always use their own login-keychain key
(`io.github.lawkitt.mdoc.dev`) and `mdoc/cases-dev/`, so they never touch
release cases.

Before publishing a release that changes case-key handling, install it on a
clean macOS user account and run `scripts/keychain-launch-check.sh`. It seeds
a key this build cannot read silently and checks that launch shows no
Keychain dialog and leaves cases locked.

## App icon

The app icon's source is `build/appicon.svg`: a square SVG whose rounded square
fills the canvas. After changing it, run `just icon` on macOS (built-in `sips`
and `iconutil`, Python 3 without packages). It renders every size from the
vector: the macOS ICNS with the shape at 824 of 1024 px on a transparent
canvas, as Apple's icon grid expects, and the Windows ICO and
`resources/icons/icon.png` full-bleed.
