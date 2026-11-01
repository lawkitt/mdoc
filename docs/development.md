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

Before a PR is ready, run `just lint` and the tests of the crates it changes
(`cargo nextest run -p <crate>`, the app's with `-p mdoc --features
headless-renders`, a script's suite with `bash scripts/test-<suite>.sh`;
`just test` for a change to `Cargo.lock`, the toolchain or
`.config/nextest.toml`). The checks a code PR needs green run the
full suite on both shipped OSes and the mobile jobs: `local/windows`,
`local/macos`, `local/ios` and `local/android` from [local CI](#local-ci), and
hosted `lint (ubuntu)`. A PR from a fork, or opened by a bot, gets the full
hosted matrix instead.

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
  its CHANGELOG entry as `changes/<slug>.md` (an iOS or Android app change:
  `changes/ios/<slug>.md` or `changes/android/<slug>.md`)
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
| `src/workspace/{tab_set,sidebar,focus_regions,notices,session_writer,docx_queue,quit_flow,case_coordinator,user_lists}.rs` | The window's owners: tabs and the active tab, the sidebar and its reveal, F6 regions, the bell's notices, session saves, the DOCX worker queue, closing and quitting, case coordination, and reading and saving the user-wide lists |
| `src/workspace/tab_menu.rs` | The sidebar tab menu's file and close rows: Reveal in Finder (Show in Explorer), Copy path, Close tab and Close others |
| `src/settings_ui.rs`, `src/settings_ui/` | Settings categories, save-on-change rows, model actions, the Spelling dictionary drill-in ([ADR 0076](adr/0076-settings-categories.md)) and Detection's user-wide Always and Never lists (`lists.rs`, [ADR 0092](adr/0092-recognition-correction-pipeline.md)) |
| `src/workspace/folder_sheet.rs`, `src/workspace/folder_sheet_ui.rs` | The folder sheet: a dropped folder's files, ticks and 20-file cap, and the queue of folders waiting their turn; the dialog and its case actions ([ADR 0075](adr/0075-folder-sheet.md)) |
| `src/workspace/case_page.rs` | The transient case page: cached overview, documents, names, colour, detection categories, Always and Never lists and case actions ([ADR 0077](adr/0077-case-page.md)) |
| `src/document_view.rs`, `src/document_view/` | One tab's document view: editor, source preview, import/OCR state, actions and toolbar chrome |
| `src/document_view/case_link.rs` | The tab's case-link owner: its case, the case's identities, skipped categories and lists, what the tab put on those lists, and the pending save into the case |
| `src/document_view/document_lifecycle.rs` | The tab's lifecycle owner: loading, availability, source-only, unsaved changes, untouched conversions and placeholders, and its open dialog |
| `src/document_view/import_flow.rs` | The tab's conversion owner: the import job and slot, automatic conversion, OCR consent, recognition languages and setup |
| `crates/mdoc-store` | GUI-free storage: app paths, atomic document saves and document identity, the session manifest, recents, settings types and files, encrypted saved cases and the user-wide lists sealed beside them (`lists`), crash-report masking |
| `crates/mdoc-import` | GUI-free import: library integration and pending jobs (`import`, `import_session`), explicit verified setup and offline OCR (`ocr`), OCR that keeps finished pages for the mobile apps (`resume`), model downloads and the one-at-a-time model permit, and the supervised DOCX preview worker (`docx_preview`; its entry point stays in `src/main.rs`) |
| `src/models.rs` | What setup and Settings show for each downloadable model, from the engine that owns it |
| `crates/mdoc-import/src/recognition.rs` | Recognition languages: a document's default list, its reason and the keyboard input-source signal ([ADR 0066](adr/0066-recognition-language-defaults.md)) |
| `src/preview.rs` | Preview ownership |
| `crates/mdoc-import/src/docx_preview/comments.rs`, `src/comment_panel.rs` | Read-only DOCX comments |
| `src/markdown_search.rs`, `src/search_session.rs` | Markdown find controls and revision-aware search scheduling |
| `src/pii.rs`, `src/pii/ui*` | Pseudonymization UI: scan/edit controller, Replacements panel and word popup, UI regression tests |
| `crates/mdoc-detect` | GUI-free offline detection: the GLiNER2 detector and its verified setup, split into the model stage (raw entities) and the correction stage (`correct`: the shape filter and the structured rules, ADR 0092); the user's lists, the last scan-time stage (`lists`, ADR 0092); recognition fixtures, snapshots and scoring (`recognition`, the `recognize` example, ADR 0093) |
| `src/images.rs`, `src/style.rs` | Document-relative local images and app styling |
| `src/document_view/tests.rs`, `src/workspace/tests.rs` and their `tests/` folders, `src/perf_tests.rs` | Headless flows, one file per owner beside shared helpers, ownership/lifetime checks, and opt-in performance measurements |
| `src/workspace/headless_renders.rs`, `src/workspace/headless_renders/`, `tests/headless_renders.rs`, `tests/headless_screens.txt` | Dev-only offscreen screen/Tab evidence: the driver, one scene module per feature, the output contract test and its screen list (one name per line, `merge=union`) |
| `crates/mdoc-editor` | Host-agnostic WYSIWYG and rendered-text search; maps `mdoc-markdown`'s spans to GPUI styles |
| `crates/mdoc-markdown` | GUI-free Markdown recognition shared with the mobile apps: inline spans over source byte ranges and which bytes are markers, line prefixes (headings, quotes and alerts, lists and tasks) and GFM tables ([ADR 0085](adr/0085-mobile-apps.md)) |
| `crates/mdoc-pii`, `crates/mdoc-history` | GUI-free pseudonymization review model and the plain edit transactions it follows |
| `apps/ios` | The iOS app ([ADR 0085](adr/0085-mobile-apps.md)): SwiftUI screens over the Rust core through `mdoc-ffi`'s Swift bindings (the `MdocFFI` framework), and the Restore with mdoc share extension ([README](../apps/ios/README.md)) |
| `apps/android` | The Android app ([ADR 0089](adr/0089-android-app.md)): Jetpack Compose over the Rust core through `mdoc-ffi`'s Kotlin bindings ([README](../apps/android/README.md)) |
| `crates/gpui-pdf`, `crates/gpui-bidi` | Virtualized PDF preview and bidirectional text layout |
| `src/system_quit.rs` | macOS: a quit from the Dock, an Apple Event or logout runs Quit's questions first and answers the vendored `applicationShouldTerminate:` hook when they end ([ADR 0071](adr/0071-one-close-and-quit-prompt.md)) |
| `src/shelf.rs`, `src/shelf/` | The menu bar shelf's GPUI-free model: readiness, the list, pseudonymized rows, Copy for AI text, temporary exports and Restore Clipboard ([ADR 0084](adr/0084-menu-bar-shelf.md)) |
| `src/menu_bar.rs`, `src/shelf_ui.rs`, `src/shelf_ui/` | macOS: the menu bar item and staying in the menu bar (hide on close, the Dock, the shelf popup under the item, the first-close note), and the shelf's view: rows, the Clipboard section and the footer ([ADR 0084](adr/0084-menu-bar-shelf.md)) |
| `src/workspace/shelf_source.rs` | What the window gives the shelf: open tabs and closed case members, and the ready-text fingerprint recorded in the case on save |
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
| `just recognize <file.md\|folder> [--gold] [--snapshot]` | [Recognition fixtures](../tests/fixtures/recognition/README.md) (ADR 0093): a file's proposals marked like gold, or with `--gold` its misses, false hits and boundary errors against its status; the Mac's installed FP16 model (read only), or committed snapshots with `--snapshot`. `--scoreboard [<folder>]`: recall and precision per language × category × kind and counts per status; `--refresh-snapshots` rewrites snapshots (Mac); `--record-flaws` writes known-flaw fixtures' `expect:` lines; `--check` is the detection gate. Any folder of fixtures works (the holdout) |
| `just detection-gate` | macOS: `just recognize --check` when the change touches recognition or what it depends on (`MDOC_DETECTION_GATE=always` or `never` overrides); `local/macos` runs it last |
| `just headless-renders <new-directory>` | macOS/Windows: light/dark PNGs, actual Tab walks and frame costs, entirely offscreen |
| `just package` | This machine's installers into `dist/` |
| `just smoke-package` | OCR from the packaged artifact: the `.dmg`'s signed app, or the release `mdoc.exe` (release gate) |
| `just smoke-update [dmg]` | An older Developer ID signed `.dmg` updates itself to the latest release through `--mdoc-self-update` (manual, before releases that change the updater) |
| `just release <version>` | Release commit and tag (below) |
| `just ios-core` | macOS: the iOS core into `target/ios` (`MdocCore.xcframework`, Swift bindings, PDFium) |
| `just android-setup` | Windows: the pinned Android SDK (command-line tools, `adb`, build-tools, platform 36, the emulator and an API 36 x86_64 image with 16 KB pages), Temurin 21 and NDK r29 under `E:\Android` (`MDOC_ANDROID_HOME` overrides), each archive checked against its published size and SHA-1 or SHA-256; accepts the SDK licences. About 4 GB, once ([ADR 0089](adr/0089-android-app.md)) |
| `just android-emulator [--window] [phone\|tablet]` | Windows: boots a Pixel 8-sized phone (`emulator-5554`) or Pixel Tablet-sized tablet (`emulator-5556`) on the pinned API 36 image with 16 KB pages, headless unless `--window`, writing its virtual device on first use; prints the adb serial. `adb -s <serial> emu kill` stops it |
| `just android` | Windows: the Android job when the commit touches the core or android area (`scripts/ci-areas.txt`; `MDOC_ANDROID_JOB=always` or `never` overrides): `just android-core`, the app's JVM parity and unit tests, the app and test builds, then the instrumented tests on CI's own emulator (`emulator-5560`), stopped afterwards if the job booted it. Windows local CI runs it as `local/android`, with a 40-minute limit |
| `just android-survey <out> [phone\|tablet]` | Windows: the Android UI survey, `SurveyTest` on the phone or tablet emulator: every screen light and dark, the review text and Settings at the largest font, copied into the folder (tablet ones prefixed `tablet-`). Not part of local CI |
| `just android-device-test [<serial> \| <model>:<api> ...]` | Windows: the app's instrumented tests and the long-job test (`apps/android/longjobs`: the model download, text recognition and Find names off screen and after a process kill, then no crash or ANR in mdoc's exit info), on a USB phone or emulator by its adb serial, or on Firebase Test Lab phones; with no argument, the app's tests on one phone per RAM class (Pixel 6a, Pixel 8, Pixel 10 Pro) and the long jobs on a Pixel 8 and a Galaxy A55, the Spark plan's five a day. JUnit results in `target/android-device-test/<time>/`. Test Lab needs `gcloud` signed in with the project. Not part of local CI |
| `just android-core` | Windows or macOS: the Android core into `target/android` (`jniLibs/{arm64-v8a,x86_64}` with `libmdoc_ffi.so`, ONNX Runtime and PDFium; the Kotlin bindings; licences), built with the NDK's clang for API 31; fails on a library not aligned for 16 KB pages |
| `just mobile [--ffi \| --ios]` | macOS: `--ffi`, `cargo check` of `mdoc-ffi` for iOS and Android, which the Mac's local CI runs in `local/macos` after `test`; `--ios`, the iOS job when the commit touches the core or ios area (`scripts/ci-areas.txt`; `MDOC_IOS_JOB=always` or `never` overrides): `just ios-app ios-test`, shutting down the Simulator it booted, which the Mac's local CI runs as `local/ios` with a 40-minute limit. Neither: both |
| `just local-ci [command]` | The [local CI](#local-ci) runner: `install`, `status`, `stop`, `start`, `uninstall` (macOS), or `run` / `once` in the foreground |
| `just ios-app` / `just ios-test [device]` | macOS: build the iOS core, then the app, extension and tests for the Simulator / run its unit, parity and UI tests on an iOS 26.1 Simulator |
| `just ios-device-test <udid>` | macOS: the device UI tests on a connected, unlocked iPhone on Wi-Fi: builds and installs mdoc, copies the fixtures into its container with `devicectl`, runs the review flow and the long-jobs test through the real background task, and fails on a new crash report; by hand before every TestFlight build after the first ([ADR 0088](adr/0088-ios-on-real-devices.md), [README](../apps/ios/README.md#device-ui-tests)) |
| `just icon`, `just actionlint` | Icon assets; workflow lint |
| `just disk` | Size of each worktree's `target/` |
| `just retire-worktree <path>` | Remove a finished worktree and its `target/` |

Heavy, download and machine-dependent tests run only through these recipes, on
the Apple Silicon Mac and the Windows x64 PC.

`just test` runs the workspace tests with cargo-nextest, configured in
`.config/nextest.toml`: a slow-test warning at 60 s, a kill at 2 minutes, and
the render test one minute past its own watchdog (10 minutes on macOS, 20 on
Windows); no retries. Doctests, which nextest does not run, follow through
`cargo test --doc` ([ADR 0082](adr/0082-local-ci.md)). The script suites
(release, CI cache cleanup, local CI runner) need no build, so
`scripts/test.sh` runs them beside the Rust tests and prints their output
after them. Hosted CI jobs stop at
15 (lint), 20 (clippy) and 30 (test) minutes, and after each `main` run a
cache cleanup job deletes older generations of the cargo caches it saved.

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
from the selected category tab; `focus-settings-appearance` focuses System
while the run's own theme stays selected (filled).
`focus-beside-selected-tab` also checks a focused Review tab beside the
selected Share tab.
`focus-comments-toggle` uses the public DOCX comments fixture to check the
ring at the Original pane's clipped edge.
`focus-decision-row` checks a row of What you did beside done and undone entries.
`summary.json` lists these under `focus_rings`. The contract test compares
pixels after focus scrolling settles: only the 2 px accent outline changes,
preserving the control's fill and label and checking that both sides of the
ring remain visible.

Both themes cover Home, a document and its ⋯ menu, Review, the mention popup, the
selection pill and its category menu, model setup
consent, the OCR language card, all six Settings categories (General, Detection, Recognition,
Spelling, Cases and About), General with its switches off, Detection with
installed models (Remove… on the active and an inactive model) and at the end
of its scroll, the dictionary, command palette, the keyboard shortcuts
sheet, Find and Replace,
before/after, What you did with done and undone entries, Open Case, the unlocked and locked case page, case rename/categories, the Keychain explainer preview,
the folder sheet (a small folder, one over the 20-file cap, and Add to a
case), confirmations for case deletion, mapping retention, unsaved changes and model
removal, model comparison, a damaged DOCX's failed conversion and preview,
a restored AI reply, a scan under way (skeleton rows, "Scanning…",
Rescan off, a window count), and the menu bar shelf (ready, not-ready and
changed-since-review rows, a row's "Copied", the first-close note, empty,
and locked) on a stage window. The legal review uses
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

## Local CI

Local CI is the pre-merge test gate
([ADR 0082](adr/0082-local-ci.md),
[ADR 0091](adr/0091-mobile-ci-and-releases.md)): `scripts/local-ci.sh` runs
on the Mac and on the Windows PC. The Mac posts `local/macos` (lint, test,
the mobile cargo check) and `local/ios` (the iOS job) on each PR head; the
PC posts `local/windows` (lint, test) and `local/android` (the Android job).
When a runner is stuck, the hosted fallback posts those statuses instead
(below). `scripts/test-local-ci.sh` and `scripts/test-local-ci-fallback.sh`
(part of `just test`) check both against fakes.

**Areas.** `scripts/ci-areas.txt` is the one path table: each line names an
area (`none`, `ios`, `android`, `core`, `desktop`, or two of them) and a
glob; a file takes the first line that matches it, desktop when none does,
and a core change runs every area. Its comment says which scripts are
desktop, shared (core) or mobile. `scripts/ci-areas.sh [<from> <to>]`
prints the areas a change touches (`HEAD^1` to `HEAD` by default: on a
merge ref, the PR). A new script outside `src/` and `tests/` that a mobile
job runs needs a line there, or it counts as desktop. `ci.yml`'s
`paths-ignore` repeats the `none` lines word for word (the suite checks).
Only exceptions precede the `none` lines: Markdown a test reads under
`tests/fixtures/` (the recognition fixtures, core). Local CI runs them;
hosted CI still skips them, since `paths-ignore` takes no `!` and a PR's
hosted run is only lint, which reads no fixture.

**Hosted CI on PRs.** A PR from a branch in `lawkitt/mdoc` opened by a user
runs only `lint (ubuntu)` on `pull_request`; its `areas`, `clippy` and `test`
jobs show as skipped. A merge needs `local/windows`, `local/macos`,
`local/ios`, `local/android` and `lint (ubuntu)` green. Fork and bot
(Dependabot) PRs, which the runners never test, run the full hosted matrix,
so third-party code is never compiled on the maintainer's machines before
review. `main` runs the full matrix after each merge, and `release.yml`
calls the full `ci.yml`; there the `areas` job skips `clippy` and `test`
when the change is outside the desktop and core areas (an app-only merge).

**What a runner does.** Every 60 s it lists the open PRs with the machine's
`gh` login, skipping drafts, forks and bot PRs (Dependabot). Each PR has two
jobs, one per status. As soon as GitHub has the merge ref, the runner reads
the PR's areas from the merge ref's own `scripts/ci-areas.txt`: a status
outside them gets `success` "Not affected: no core or ios change
(main@1a2b3c4)", a PR whose files are all in no area gets `success` "Docs
only, not tested" on both, and a conflicting PR gets `failure` ("Conflicts
with main"), all without a build. Any other status without this machine's
result gets `pending` "Queued", linked to the runner's `status.txt`; one job
runs at a time, oldest first, a PR's desktop job before its mobile one, so
the desktop status never waits for the mobile job. Each step of a running
job posts `pending` "Running just test on main@1a2b3c4", so GitHub shows the
step and a status older than the longest step limit means the runner
stopped. A job fetches `refs/pull/N/merge`, checks it out in the CI checkout
(keeping its `target/`) and installs the merge ref's pinned tools with
`scripts/setup.sh` (as hosted CI's setup step does). The desktop job then
runs `just lint` and `just test` with the hosted job limits (20 and 30
minutes), and on the Mac `just mobile --ffi` (20 minutes) and the
[detection gate](../tests/fixtures/recognition/README.md) `just
detection-gate` (30 minutes; its model run has a 5-minute budget and reads
the pinned FP16 from the runner's app data, never downloading). The hosted
fallback cannot run the gate (no model). The mobile job
runs `just mobile --ios` on the Mac or `just android` on the PC (40
minutes each, [ADR 0089](adr/0089-android-app.md)). A runner reads its own
copy of `local-ci.sh` and `lib.sh`, so a change to them takes effect after
the runner is installed again. The runner's `PATH` puts `~/.cargo/bin`
first, so the pinned tools win over other installs. The status, `success`
or `failure`, names the base commit tested ("Passed on main@1a2b3c4 in
4m10s") and links to the job's folder (`pr-N/<head>/`, the mobile job's
`pr-N/<head>-ios/` or `-android/`). A newer head ends its PR's queued or
running jobs (`error`, "Superseded by …"); a pending status left by a
stopped runner runs again when it starts.

**Testing `main`.** When no PR job is queued, the runner takes the `main`
commits after the last one it queued (`state/main-last`), oldest first, and
tests each one in the areas it touched, against the commit before it,
posting both statuses on that commit ("Passed on main@<commit>"; "Not
affected" and "Docs only" as on PRs); results are under `out/main/`. On its
first run it takes `main`'s tip only, and after a long time off the newest
10 (`LOCAL_CI_MAIN_MAX`). A PR head that arrives waits for a running `main`
job, then goes first. Queued `main` jobs live in `state/main-queue` and run
again after a restart. A red local status on `main` stops merges, as a red
hosted `main` does. On the Mac a job starts only while memory
pressure is normal (`kern.memorystatus_vm_pressure_level` 1) and 40 GB is
free. Not swap in use: macOS keeps swap after memory is free again, and a
2 GB swap limit held every job from 11:58 to 15:49 UTC on 2026-10-31 with
nothing on GitHub to say why. While the Mac waits, queued heads say so
("Queued; waiting for memory pressure to drop (now warning)"), as do
`status.txt` and the log.

**Machine settings** go in the runner home's `runner.env` (`~/ci/runner.env`,
`E:\ci\runner.env`), after the three lines install writes; install and the
setup script keep them. Never in the launchd plist or the task, which they
rewrite. The settings are the `LOCAL_CI_*` variables in `config()` in
`scripts/local-ci.sh`, for example `LOCAL_CI_MIN_FREE_GB=30` or
`LOCAL_CI_MAX_PRESSURE=2` (start at warning pressure too). Restart the runner
after a change.

| | Mac | Windows PC |
|---|---|---|
| Runner home | `~/ci` | `E:\ci` (the Dev Drive) |
| CI checkout | `~/ci/mdoc` | `E:\ci\mdoc` |
| Runner (a copy, never the checkout it tests) | `~/ci/runner/scripts/` | `E:\ci\runner\scripts\` |
| Results, kept 7 days | `~/ci/out/pr-N/<head>/` | `E:\ci\out\pr-N\<head>\` |
| Runner log | `~/ci/state/runner.log` | `E:\ci\state\runner.log` (start-up errors: `task.log`) |
| Started by | launchd agents `com.lawkitt.mdoc.local-ci` and `com.lawkitt.mdoc.local-ci-web` | Task Scheduler tasks `mdoc local CI` and `mdoc local CI web`, at logon, normal priority |

Each machine serves its `out` folder to the tailnet only, at
`https://<machine>.<tailnet>.ts.net/ci/`; the status links open a job's
folder there: `log.txt` (checkout and both steps), `summary.txt` (PR, head,
base, step times, result) and `renders/` (the headless render set:
`summary.json`, `light/`, `dark/`; about 0.5 GB on macOS, so only a PR's
newest tested head keeps them). `status.txt` at the top is the runner's
heartbeat, current job and queue. The macOS Tailscale app cannot serve a
folder, and on Windows serving one needs an admin, so on each machine a
localhost-only Python `http.server` (the `local-ci-web` agent, the `mdoc
local CI web` task) serves it and `tailscale serve` proxies `/ci` to it.

**Install or update.** On the Mac, from a checkout of current `main`:
`just local-ci install`. On the PC, in PowerShell (not as admin) from a
checkout, with Python on `PATH`:
`powershell -ExecutionPolicy Bypass -File scripts\local-ci-setup.ps1`. Both
clone the CI checkout if it is missing (its first build is cold and its
`target/` reaches about 30 GB), copy the runner, register the agents or the
task and add `/ci` to `tailscale serve`; existing `tailscale serve` paths are
kept. Re-run the same command after `scripts/local-ci.sh` or
`scripts/lib.sh` changes on `main`; nothing updates the runner by itself
(on 2026-11-01 the Mac still ran a copy from before the swap-gate fix and
held PRs for it). When its copy differs from `main`'s, `status.txt` says
so ("outdated: this runner's local-ci.sh differ from main's; re-run
install") and the log warns.
The area table is not copied: the runner reads it from each commit it
tests. Never run either from the CI checkout, which sits at whatever merge
ref it tested last.

**Check the runners** (the fleet preflight). Open `/ci/status.txt` on each
machine over the tailnet: an `updated` time within the last 3 minutes means
that runner is up. On the machine, `just local-ci status` shows the agents
or the task, the status file and the last log lines, and exits 1 when the
runner is not running.

**Stop and start.** Mac: `just local-ci stop` and `just local-ci start`.
PC: `Stop-ScheduledTask 'mdoc local CI'` and `Start-ScheduledTask 'mdoc
local CI'` (re-run the setup script to also end a job's build processes). A
job in progress stops and runs again at the next start. `just local-ci
uninstall` (Mac) or `local-ci-setup.ps1 -Uninstall` removes the agents or
the tasks and the `/ci` path and keeps the runner home.

**Run a head again.** Post a pending status on it; the runner queues any
pending head it is not running:

```sh
gh api -X POST repos/lawkitt/mdoc/statuses/<sha> -f state=pending -f context=local/macos -f description=Queued
```

(`context=local/ios` or `local/android` for a mobile job.)

**Hosted fallback.** `.github/workflows/local-ci-fallback.yml` runs
`scripts/local-ci-fallback.sh` every 10 minutes and after each CI run. It
looks at the heads the runners test (branches in `lawkitt/mdoc`, opened by a
user, not drafts) and calls a context stuck when it has

| Status on the head | for at least | which means |
|---|---|---|
| none | 15 min after the head's `lint (ubuntu)` started | the runner is off or asleep |
| "Queued; waiting for …" | 15 min | the runner is up but out of capacity |
| "Running …" | 50 min (the longest step limit is 40) | the runner stopped mid-job |
| any other `pending` ("Queued") | 45 min | a long queue, or the runner is gone |

For a stuck head it dispatches the full `ci.yml` on the PR branch (one
dispatch serves both contexts) and posts `pending` "Hosted fallback: hosted
CI on 1a2b3c4 (attempt 1); …", linked to the branch's CI runs. When that
run's `clippy` and `test` jobs for the same OS finish, it posts their result
under the context: "Hosted fallback: clippy and test (macos-26) passed; just
mobile not run" (hosted CI has no `just mobile`), or `failure` naming the
job. A dispatch with no result in 25 minutes (cancelled or missing), or
whose jobs are still queued after 60, is retried, three dispatches in all,
then the context gets `error`; a retry waits while the head's hosted run is
still going, and a newly stuck context joins that run. A stuck `local/ios`
or `local/android` dispatches nothing: until the hosted mobile jobs exist
([ADR 0091](adr/0091-mobile-ci-and-releases.md) item 5) it gets `success`
"Hosted fallback: NOT tested; no hosted iOS job yet (ADR 0091 item 5)" at
once, so a machine that is off never blocks merges (temporary). The merge gate does
not change: a green fallback status counts like a local one. A runner that
comes back queues every head with a fallback status, pending or finished,
and its own result replaces the fallback's. The fallback writes only over
the status it read, re-reading it right before each write, so a runner's
result wins (a runner posting in that same second is the one window left). The workflow runs
`main`'s script with its own token (`statuses: write`, `actions: write`) and
never checks out PR code; the dispatched `ci.yml` run tests the branch head,
not the merge ref, with its read-only permissions.

**Run the fallback now.** `gh workflow run local-ci-fallback.yml --repo
lawkitt/mdoc` runs one pass; it still waits for the times above.

**Dispatch by hand.** Run the full hosted matrix on the PR branch and read
the result from the head commit's check runs: a dispatched run is not in the
PR's checks (`gh pr checks`, `statusCheckRollup`, checked on #253), and only
a full run names `test (windows-latest)` and `test (macos-26)`:

```sh
gh workflow run ci.yml --repo lawkitt/mdoc --ref <branch>
head=$(gh pr view <PR> --repo lawkitt/mdoc --json headRefOid -q .headRefOid)
gh api "repos/lawkitt/mdoc/commits/$head/check-runs?check_name=test%20(windows-latest)" \
  --jq '.check_runs[] | [.status, .conclusion, .html_url] | @tsv'
```

A new head needs a new dispatch.

Runners execute PR code under the maintainer's account with their `gh`
login; only branches pushed to `lawkitt/mdoc` by someone with write access
are tested, and the results are reachable only inside the tailnet.

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
  reuse lanes instead of a new worktree per task: `windows-lane-1` to
  `windows-lane-4` on `windows`, `wsl-lane-1` on `wsl`.
  Each task starts with `git switch -c <machine>/<topic> origin/main`.
  Retire other worktrees with `just retire-worktree <path>` once merged.
  On `windows` everything mdoc builds lives on `E:`, a 400 GB ReFS Dev Drive
  (a dynamic VHDX at `D:\DevDrive\dev.vhdx`, attached at startup by the
  `SYSTEM` task `Mount Dev Drive`; fleet ADR 0019): the root checkout
  `E:\Developer\lawkitt\mdoc`, the lanes `E:\t3\worktrees\mdoc\windows-lane-N`
  (worktrees of the root checkout) and the local CI home `E:\ci`.
  `CARGO_HOME` and `RUSTUP_HOME` stay on `C:`. Claude Desktop SSH sessions
  use these lane folders too (`use_worktree: false`), one session or T3
  thread per lane.
- **Design tripwire.** A lane stops and reports to the hub before adding a
  dependency, vendoring code, patching `vendor/` or a GPUI platform crate, or
  editing outside its item's area (fleet ADR 0019).
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
- **Local CI** runs on the Mac and the PC ([Local CI](#local-ci)); the Mac
  runner is a build slot beside the two agent builders. Preflight: each
  machine's `/ci/status.txt` updated within the last 3 minutes. UI items'
  renders are in the `local/macos` and `local/windows` status links.
- **Required checks:** `local/windows`, `local/macos`, `local/ios`,
  `local/android` and `lint (ubuntu)`; the lane waits for them and fixes
  failures, and the hub merges on the same five. They run the full suite on
  both shipped OSes and the mobile jobs, so a lane's gate is
  the fast subset under [Build from source](#build-from-source): `just lint`
  and the changed crates' tests (`wsl` keeps its own gate). When a runner
  is stuck or its machine is off, the hosted fallback posts its status
  ("Hosted fallback: …", [Local CI](#local-ci)); a green one counts.
- **Native checks.** The [acceptance](platform-acceptance.md) checklist runs
  by computer use (see [ADR 0057](adr/0057-native-platform-acceptance.md));
  rows that need a physical keyboard, speech, passwords or system settings
  stay with the maintainer. During a milestone the hub runs GUI checks on a
  dev build (fleet ADR 0018): a dev build uses the real profile, so copy
  `~/Library/Application Support/mdoc` aside first and restore
  `session.json` and `recents.json` after each check, never opening
  fixtures in a case. Computer use grants a bundle, so the hub wraps the
  build as `~/Applications/mdoc-dev.app` (bundle id
  `io.github.lawkitt.mdoc.dev`), grants it at preflight, and copies each
  branch's binary into that same bundle.
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
# A fast subset (Build from source): the app is `-p mdoc --features
# headless-renders`, a script's suite `bash scripts/test-<suite>.sh`, and a
# change to Cargo.lock, the toolchain or .config/nextest.toml runs `just test`.
gate = ["just lint", "cargo nextest run -p <each changed crate>"]
checks = ["local/windows", "local/macos", "local/ios", "local/android", "lint (ubuntu)"]
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
fetch = "open the `local/macos` and `local/windows` status links (tailnet): `renders/light/` and `renders/dark/`"
done_when = "headless renders of {what}, light and dark (local CI serves them)"

[bookkeeping]
roadmap_item = "remove item {number} from `{roadmap}` {milestone} (delete only its own lines; do not renumber)"
issue = "the commit that finishes it says `fixes #{number}`"
fragment = "add `changes/{slug}.md`, or `changes/ios/{slug}.md` / `changes/android/{slug}.md` for an app change (format in `changes/README.md`)"

[machines.macbook-air]
setup = "Never set `CARGO_TARGET_DIR`: this worktree builds into its own `target/` (a shared one gives stale builds). First build is cold."

[machines.windows]
disk_check = "`Get-PSDrive E` (not `just disk`, which fails while another lane builds)"
lanes = { windows-lane-1 = 'E:\t3\worktrees\mdoc\windows-lane-1', windows-lane-2 = 'E:\t3\worktrees\mdoc\windows-lane-2', windows-lane-3 = 'E:\t3\worktrees\mdoc\windows-lane-3', windows-lane-4 = 'E:\t3\worktrees\mdoc\windows-lane-4' }
notes = "Every lane, the root checkout and local CI's home are on `E:`, a ReFS Dev Drive (a VHDX on `D:`); `CARGO_HOME` and `RUSTUP_HOME` stay on `C:`."

[machines.wsl]
disk_check = "`df -h ~`"
lanes = { wsl-lane-1 = "~/.t3/worktrees/mdoc/wsl-lane-1" }
gate = ["cargo test -p mdoc-pii -p mdoc-editor -p mdoc-history"]
notes = "Only the platform-neutral crates build here; a Linux build of the app is not a shipped target."
```

## Releasing

Releases ship only for Apple Silicon macOS (`.dmg`) and Windows x64 (NSIS
`.exe` only, no `.msi`: [ADR 0083](adr/0083-fewer-release-candidates.md)). The
repository version is the source of truth: `just release 0.1.0-beta.1` checks
that `main` is clean and current, runs the checks (optionally `check-all`),
sets the version in `Cargo.toml`,
`Cargo.lock` and `Info.plist`, moves the `changes/` fragments into the
CHANGELOG's Unreleased section, names that section for stable versions,
commits `release: v…`, tags, and pushes after confirmation. The tag runs
`.github/workflows/release.yml`: version check, CI with the OCR smoke test on
both targets, packaging and the packaged-app gate, then a draft GitHub Release
with `SHA256SUMS` and build provenance. Review the draft and publish it by
hand. "Run workflow" on the Release workflow builds an unpublished test build
from any pushed branch; it goes straight to packaging (the branch already ran
CI) unless **checks** is ticked, and signs the macOS app only with **sign**,
which works from `main` only: the `release` environment admits `main` and `v*`
tags ([ADR 0091](adr/0091-mobile-ci-and-releases.md)).

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
