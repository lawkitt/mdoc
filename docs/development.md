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
`.config/nextest.toml`). A PR head needs `lint (ubuntu)` and `merge-gate`
green (the fast tier, and the detection gate for a recognition change);
the full suite on both shipped OSes and the mobile
jobs runs in the merge queue ([CI](#ci)).

## Project documentation

- [Roadmap](../ROADMAP.md): open work by milestone; bugs are [Issues](https://github.com/lawkitt/mdoc/issues).
- [Glossary](../CONTEXT.md): product and code terminology.
- [Architecture decisions](adr/): accepted behavior and its rationale.
- [Evidence](evidence/): measurements that current work still compares against.
- [Mobile UI guide](mobile-ui.md): the mobile apps' tokens, components, gestures and how to change the UI.
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
| `crates/mdoc-detect` | GUI-free offline detection: the GLiNER2 detector and its verified setup, split into the model stage (raw entities) and the correction stage (`correct`: the rule pack's shape filter, `span` fixes and `context` rules, and the structured rules, ADR 0092); the user's lists, the last scan-time stage (`lists`, ADR 0092); recognition fixtures, snapshots and scoring (`recognition`, the `recognize` example, ADR 0093) |
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
| `just test-tier <tier>` | One test tier (ADR 0095): `unit`, `app`, `renders`, `recognition`, `scripts` or `quarantined` |
| `just smoke-ocr` | Downloads the pinned OCR runtime and recognizes the EN/RU fixtures (release gate) |
| `just smoke-models [--download]` | Default OCR and pseudonymization models through setup and offline paths |
| `just check-all` | `check` + both smoke recipes |
| `just perf` | Ignored performance tests, serially |
| `just recognize <file.md\|folder> [--gold] [--snapshot]` | [Recognition fixtures](../tests/fixtures/recognition/README.md) (ADR 0093): a file's proposals marked like gold, or with `--gold` its misses, false hits and boundary errors against its status; the Mac's installed FP16 model (read only), or committed snapshots with `--snapshot`. `--scoreboard [<folder>]`: recall and precision per language × category × kind and counts per status; `--refresh-snapshots` rewrites snapshots (Mac); `--record-flaws` writes known-flaw fixtures' `expect:` lines; `--check` is the detection gate (`--shard k/n` for one CI shard, `--out <dir>` for its regenerated snapshots); `--setup` installs the pinned FP16 through mdoc's verified setup (CI's runner, ADR 0095). Any folder of fixtures works (the holdout) |
| `just import <file>` | What mdoc's own import makes of a file, as Markdown on stdout (warning and errors on stderr); a PDF needing OCR uses the installed OCR models (read only; `MDOC_OCR_ROOT` points elsewhere). Judge probes on this text, not on another converter's (ADR 0093) |
| `just snapshots-fetch <pr\|run>` | A failing `ci/detection` run's regenerated snapshots (the PR's newest run with them, or that run) into the tree, to review and commit (ADR 0095) |
| `just detection-gate` | macOS: `just recognize --check` when the change touches recognition or what it depends on (`MDOC_DETECTION_GATE=always` or `never` overrides), on this Mac's scores; CI runs `just recognize --check` in shards on hosted `macos-26` instead (`ci/detection`), whose snapshots differ in the fourth decimal place |
| `just headless-renders <new-directory>` | macOS/Windows: light/dark PNGs, actual Tab walks and frame costs, entirely offscreen |
| `just package` | This machine's installers into `dist/` |
| `just smoke-package` | OCR from the packaged artifact: the `.dmg`'s signed app, or the release `mdoc.exe` (release gate) |
| `just smoke-update [dmg]` | An older Developer ID signed `.dmg` updates itself to the latest release through `--mdoc-self-update` (manual, before releases that change the updater) |
| `just release <version>` | Release commit and tag (below) |
| `just ios-core` | macOS: the iOS core into `target/ios` (`MdocCore.xcframework`, Swift bindings, PDFium) |
| `just android-setup` | Windows, macOS or Linux: the pinned Android SDK (command-line tools, `adb`, build-tools, platform 36, the emulator and an API 36 image with 16 KB pages, x86_64, or arm64-v8a on the Mac), Temurin 21 and NDK r29 under `E:\Android`, `~/Library/Android` or `~/Android` (`MDOC_ANDROID_HOME` overrides), the SDK in `Sdk` there, or `mdoc-sdk` on macOS and Linux, apart from Android Studio's (a folder there it did not install stops it), each archive checked against its published size and SHA-1 or SHA-256; accepts the SDK licences. About 4 GB, once ([ADR 0089](adr/0089-android-app.md)) |
| `just android-emulator [--window] [phone\|tablet]` | Windows, macOS, or Linux with read and write access to `/dev/kvm`: boots a Pixel 8-sized phone (`emulator-5554`) or Pixel Tablet-sized tablet (`emulator-5556`) on the pinned API 36 image with 16 KB pages, headless unless `--window`, writing its virtual device on first use; prints the adb serial. `adb -s <serial> emu kill` stops it |
| `just android` | Windows, macOS, or Linux with KVM: the Android job when the commit touches the core or android area (`scripts/ci-areas.txt`; `MDOC_ANDROID_JOB=always` or `never` overrides): `just android-core`, the app's JVM parity and unit tests, the app and test builds and the minified release-like app (the `minified` build type: R8 with `apps/android/app/proguard-rules.pro`, debug-signed, `io.github.lawkitt.mdoc.minified`); on CI's own emulator (`emulator-5560`), stopped afterwards if the job booted it, the minified app must open its library on Home (a missing keep rule fails here, [ADR 0091](adr/0091-mobile-ci-and-releases.md)), then the instrumented tests run. The PC's queue runner runs it as `ci/android`, with a 40-minute limit |
| `just ios-survey <out> [device]` | macOS: the iOS UI survey, `SurveyUITests` on the iPhone 17 Pro (or the device given): every screen light and dark into the folder, usually `docs/evidence/ios-app/current` ([mobile UI guide](mobile-ui.md)). Shuts down only its Simulator. Not part of CI |
| `just android-survey <out> [phone\|tablet]` | Windows: the Android UI survey, `SurveyTest` on the phone or tablet emulator: every screen light and dark, the review text and Settings at the largest font, copied into the folder (tablet ones prefixed `tablet-`). Not part of CI |
| `just android-device-test [<serial> \| <model>:<api> ...]` | Windows: the app's instrumented tests and the long-job test (`apps/android/longjobs`: the model download, text recognition and Find names off screen and after a process kill, then no crash or ANR in mdoc's exit info), on a USB phone or emulator by its adb serial, or on Firebase Test Lab phones; with no argument, the app's tests on one phone per RAM class (Pixel 6a, Pixel 8, Pixel 10 Pro) and the long jobs on a Pixel 8 and a Galaxy A55, the Spark plan's five a day. JUnit results in `target/android-device-test/<time>/`. Test Lab needs `gcloud` signed in with the project. Not part of CI |
| `just android-core` | Windows, macOS or Linux: the Android core into `target/android` (`jniLibs/{arm64-v8a,x86_64}` with `libmdoc_ffi.so`, ONNX Runtime and PDFium; the Kotlin bindings; licences), built with the NDK's clang for API 31; fails on a library not aligned for 16 KB pages |
| `just mobile [--ffi \| --ios]` | macOS: `--ffi`, `cargo check` of `mdoc-ffi` for iOS and Android, which `ci/macos` runs; `--ios`, the iOS job when the commit touches the core or ios area (`scripts/ci-areas.txt`; `MDOC_IOS_JOB=always` or `never` overrides): `just ios-app ios-test`, shutting down the Simulator it booted; `ci/ios (tests)` runs those recipes after the core, with a 40-minute limit. Neither: both |
| `just local-ci [command]` | The retired local CI runner (ADR 0082): `uninstall` and `status` only matter now; the scripts go with CI redesign item 7 |
| `just ios-app` / `just ios-test [device]` | macOS: build the iOS core, then the app, extension and tests for the Simulator / run its unit, parity and UI tests on an iOS 26.2 Simulator (Xcode 26.2) (`MDOC_IOS_TEST_WORKERS=<n>`: on n clones of it, as `ci/ios` does) |
| `just ios-release` | macOS: the iOS core, then the app and extension in the Release configuration for the Simulator, unsigned, without tests: what an archive compiles; `ci/ios-release` runs it |
| `just ios-device-test <udid>` | macOS: the device UI tests on a connected, unlocked iPhone on Wi-Fi: builds and installs mdoc, copies the fixtures into its container with `devicectl`, runs the review flow and the long-jobs test through the real background task, and fails on a new crash report; by hand before every TestFlight build after the first ([ADR 0088](adr/0088-ios-on-real-devices.md), [README](../apps/ios/README.md#device-ui-tests)) |
| `just icon`, `just actionlint` | Icon assets; workflow lint |
| `just disk` | Size of each worktree's `target/` |
| `just retire-worktree <path>` | Remove a finished worktree and its `target/` |

Heavy, download and machine-dependent tests run only through these recipes, on
the Apple Silicon Mac and the Windows x64 PC.

`just test` runs the workspace tests with cargo-nextest, configured in
`.config/nextest.toml`: a slow-test warning at 30 s, a kill at 2 minutes, the
app package's tests at most 4 at a time, and the render test one minute past
its own watchdog (10 minutes on macOS, 20 on Windows); no retries. A slow
test is a bug to fix, not a limit to raise. Doctests, which nextest does not
run, follow through `cargo test --doc` ([ADR 0082](adr/0082-local-ci.md)).
The script suites (release, CI cache cleanup, local CI runner, the quarantine
list) need no build, so `scripts/test.sh` runs them beside the Rust tests and
prints their output after them.

`just test-tier <tier>` runs one test tier, as CI's jobs will
([ADR 0095](adr/0095-ci-on-a-merge-queue.md)): `unit` (the crates, without
the recognition fixtures, and the doctests), `app` (the app package without
the render test), `renders`, `recognition` (the fixtures on their
snapshots), `scripts` (the suites and the quarantine check), or
`quarantined`. Tests never retry. A test that flakes twice goes into
`.config/quarantine.txt` (`<nextest filter>  #<issue>  <expires
YYYY-MM-DD>`) by PR, with its issue: every tier leaves it out, `just
test-tier quarantined` runs the list alone without failing, and a line past
its date fails the `scripts` tier until the test is fixed or the date is
renewed in a PR that says why. Hosted CI jobs stop at
15 (lint), 20 (clippy) and 30 (test) minutes, and after each `main` run a
cache cleanup job deletes older generations of the cargo caches and the
built iOS core.

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

## CI

`main` merges through GitHub's merge queue
([ADR 0095](adr/0095-ci-on-a-merge-queue.md)). A PR head runs a fast tier
on hosted Linux; the full suite runs once per queue group, on hosted
runners and the Windows PC. The Mac is not a CI runner. ADR 0082's local
CI and its hosted fallback are retired.

**Required checks.** A ruleset on `main` requires `lint (ubuntu)` and
`merge-gate`, on the PR head and on the queue commit; the maintainer's
bypass is for `just release` only. Never merge with `--admin`.

| Commit | What runs | `merge-gate` is green when |
|---|---|---|
| PR head | `lint (ubuntu)` and `fast (ubuntu)`, the summary of `fast (unit)` (`just test-tier unit`) and `fast (scripts)` (`just test-tier scripts`), two parallel jobs; on a recognition-area PR also the detection gate, `detection-head / ci/detection` | `fast (ubuntu)` passed ("the full suite runs in the merge queue"), and on a recognition-area PR the detection gate too; or the PR is docs only |
| Queue commit (`gh-readonly-queue/main/*`) | `queue.yml`'s hosted jobs and the PC's two jobs, each when the commit's areas need it | every job it needs passed; the description names each and where it ran. A hosted job that never ran is re-run first (see the gate's passes) |

**Merging.** When a PR's checks are green and it is reviewed, enqueue it:
`gh pr merge <N> --auto` (the queue sets the method: squash). The queue builds groups of 1 to 4 PRs
after a 5-minute wait, two groups at a time, each on `main` plus the PRs
ahead of it. A group whose queue commit fails is taken out; GitHub
re-tests the rest without it. The dropped PR gets a red `merge-gate` on its
queue commit, which names the failing job and links to it. Fix it on the
branch and enqueue it again.

**Independent PRs, not stacks** ([ADR 0100](adr/0100-ci-reliability-after-the-cutover.md)).
The queue takes only PRs based on `main`, so a stacked PR waits for its
parent's merge, a rebase, its fast tier and then its own queue run, one at
a time. Make independent PRs off `main` where you can, and enqueue them
together so they share a group. Keep stacks for changes that truly depend
on each other.

**A stale gate or a missed enqueue.** If `merge-gate` on a PR head still
says "Waiting" after its checks finished, or auto-merge is on and the gate
is green but the PR isn't in the queue after 5 minutes, re-check with
`gh workflow run merge-gate.yml --repo lawkitt/mdoc`. Never re-run the
PR's own `pull_request_target` gate run. If the PR still doesn't enter the
queue, turn auto-merge off and on again.

**Break-glass.** If a merged change to the gate blocks the queue, it also
blocks its own revert. The maintainer then pushes the revert directly to
`main` through the ruleset's bypass, its only use besides `just release`.
A change to what decides merges (`merge-gate.sh`, `queue.yml`, the job
workflows, `queue-runner.sh`, `ci-areas.txt`) first passes a shadow-branch
run that includes one deliberately broken case it must fail. A shadow
commit brings its whole PR onto `main`, so a PR that is also core (the
table, `lib.sh`) can stand for a ci-only change only if the trial commit
marks those files `ci` in its own table (#622). `merge-gate.yml` runs
`main`'s gate, so the gate's side of a change shows in `test-merge-gate.sh`
and a dry run of the branch's script against the trial commits, not in the
trial's own `merge-gate` status.

**Jobs on a queue commit.** `scripts/ci-areas.sh --queue` (in `queue.yml`)
diffs the commit against `main`'s merge base and picks the jobs from its
areas; `merge-gate` reads the same diff by the commit's own
`scripts/ci-areas.txt` (`main`'s when GitHub cannot give it), as the PC
does, so a skipped job is one it does not
wait for.

| Job (status) | Areas | Runs on | Limit |
|---|---|---|---|
| `ci/macos` | desktop | hosted `macos-26` (`ci-desktop.yml`), the summary of two parallel jobs: `ci/macos (checks)`, lint, the script suites under `/bin/bash` 3.2 and `just mobile --ffi`; `ci/macos (tests)`, the unit, recognition, app and renders tiers | 20 min (checks), 30 min (tests) |
| `ci/windows` | desktop | the PC (`scripts/queue-runner.sh`): setup, `just lint`, `just test` | 30 min |
| `ci/ios` | ios | hosted `macos-26` (`ci-ios.yml`), the summary of two parallel jobs: `ci/ios (tests)`, `just ios-app ios-test` on the pinned Xcode (as `just mobile --ios`), and `ci/ios-release`, `just ios-release` | 40 min (tests), 30 min (Release) |
| `ci/android` | android | the PC: `just android`; its fallback is hosted `ubuntu-latest` with KVM (`ci-android.yml`, check `ci/android (hosted)`), which `merge-gate` dispatches (below) | 40 min |
| `ci/detection` | desktop, recognition | hosted `macos-26` (`ci-detection.yml`): the detection gate in 4 shards, each warning past 80% of its 5-minute model budget | 20 min a shard (10 once `main` caches its build, item 7) |

| `ci/scripts` | ci, without desktop | hosted (`ci-scripts.yml`), the summary of two parallel jobs: `ci/scripts (linux)`, the script suites on `ubuntu-latest`; `ci/scripts (bash 3.2)`, the same under `/bin/bash` 3.2 on `macos-26`. No toolchain or cache | 10 min |

A core change needs every job; docs only needs none. A change to only the
CI machinery (the ci area) needs only `ci/scripts`, beside `lint (ubuntu)`
([ADR 0100](adr/0100-ci-reliability-after-the-cutover.md)); with a desktop
change, `ci/macos (checks)` runs the same suites instead.

**The iOS jobs.** `ci/ios` is a summary job, green only when both of its
parts ran and passed: `ci/ios (tests)` (job id `ios`) and `ci/ios-release`.
Both build the core with `just ios-core` and restore one cargo cache
(`cache-key: ios`), which `main-caches.yml`'s build-only run of the tests
job saves. Xcode needs a Simulator runtime of the SDK's version for any
app build, Release included, so each job runs `scripts/ios-toolchain.sh
--start`: it uses an installed runtime whose major.minor is
`IOS_SIM_RUNTIME`, or downloads it (about 8 GB) in the background while
the core builds; `--finish` waits for it. The pins, Xcode 26.2 and iOS
26.2, are ones the `macos-26` image ships (with its iPhone 17), so neither
job downloads a runtime (#630); a pin the image lacks costs each job about
8 GB and 4 minutes. A Mac that runs `just mobile --ios` needs the same
Xcode 26.2 (`just doctor` checks it), with its iOS 26.2 platform.
`MDOC_IOS_TEST_WORKERS` in `ci-ios.yml` switches parallel testing on
(the number of Simulator clones); it is off, as 2 clones were slower and
failed UI tests (#617).

**Areas.** `scripts/ci-areas.txt` is the one path table: each line names an
area (`none`, `ios`, `android`, `core`, `desktop`, `recognition`, `ci`, or
two of them) and a glob; a file takes the first line that matches it, desktop when
none does, and a core change runs every area. Its comment says which
scripts are desktop, shared (core) or mobile; `crates/mdoc-detect` and
`crates/mdoc-pii` are `recognition,desktop`, so they skip the mobile jobs.
`scripts/ci-areas.sh [<from> <to>]` prints the areas a change touches. A new
script outside `src/` and `tests/` that a mobile job runs needs a line
there, or it counts as desktop. The ci area is `merge-gate.sh`,
`queue-runner.sh` and its setup, the CI scripts (`ci-*.sh` but the table's
own `ci-areas.sh`, which is core; `deny-ci.sh`, `nightly-issue.sh`, the
retired local CI), their suites, and the workflows that build and test
nothing themselves (`ci.yml`, `ci-scripts.yml`, `main-caches.yml`, the
gate's two, `nightly.yml`). A workflow that builds or tests the app takes
the area of what it runs: `ci-desktop.yml`, `ci-detection.yml`, `queue.yml`
(which starts them) and `release.yml` are desktop, `ci-ios.yml` ios. When
unsure, leave a file desktop. `ci.yml`'s `paths-ignore` repeats the `none`
lines word for word (`scripts/test-local-ci.sh` checks).

**Test tiers.** `.config/nextest.toml` names them (`unit`, `app`, `renders`,
`recognition`); `just test-tier <tier>` runs one, `scripts` the script
suites, and `just test` all of them. A test on `.config/quarantine.txt`
(`<nextest filter>  #<issue>  <expires YYYY-MM-DD>`) is left out of every
tier. An Android or iOS UI test goes there as `android:<class>#<method>` or
`ios:<target>/<class>/<method>`; the Android job and `just ios-test` skip it,
and nothing runs it until the line goes (fix the test, then remove the
line). `just test-tier quarantined` runs the Rust ones alone, nightly too,
and any line past its date fails the scripts tier. A lane runs what it touched before marking a PR ready (see the
profile's `gate`) and may dispatch the full hosted desktop job on its
branch: `gh workflow run ci-desktop.yml --repo lawkitt/mdoc --ref <branch> -f os=macos`.

**Tests and OS services.** A test never asks the OS's security or identity
services: the build's code signature (`SecCodeCopySelf`), the case key
(Keychain, Windows data protection, Secret Service) or the build identity
built on them. Tests inject a case store (`Store::with_key`) and take the
build label as `Dev`. `mdoc-store`'s feature `deny-os-services`, which the
app's dev-dependencies turn on, makes those calls panic in every test
build ("a test asked the OS for …"), and `tests/os_services_guard.rs`
proves it. On macOS the signature query of an unsigned test binary read
every file in `target/debug/deps` and took seconds per test process
(#510). A new call into such a service goes behind the same guard.

**Detection gate.** Hosted `macos-26` is its reference: the snapshots
(`tests/fixtures/recognition/**/*.model.json`) are recorded there, not on a
developer Mac, whose scores differ in the fourth decimal place. A failing shard uploads the
fixtures it found stale, regenerated, as `detection-snapshots-<k>`.
`just snapshots-fetch <PR|run>` writes them into the tree; review the
diff, commit and push. Scores in CI depend on ONNX Runtime's threads,
so the shards set `MDOC_DETECT_THREADS=4`.

A recognition-area PR runs the gate on its head
([ADR 0100](adr/0100-ci-reliability-after-the-cutover.md)), so a stale
snapshot is red there, not in the queue: `ci.yml`'s `detection-head` job,
4 shards, on every push (a newer push cancels it), drafts included. Its
head `merge-gate` waits for `detection-head / ci/detection` beside the fast
tier, and when it fails says `just snapshots-fetch <PR>`. A recognition-area
PR is one with a file on a recognition line of `scripts/ci-areas.txt`
(the fixtures, `crates/mdoc-detect`, `crates/mdoc-pii`;
`head_detection` in `scripts/lib.sh`); a core or desktop change alone runs
the gate only in the queue, since hosted macOS runners are scarce. The
head run replaces a manual run of the Detection gate workflow, so lanes no
longer dispatch it. The queue still runs `ci/detection` on the queue
commit.

**Renders.** `ci/macos` uploads the full headless render set as
`renders-macos` (7 days). A UI lane dispatches the desktop job on its branch
with the screens it changed and downloads the small artifact:

```sh
gh workflow run ci-desktop.yml --repo lawkitt/mdoc --ref <branch> -f os=macos -f screens='light/home.png,*settings*'
gh run download <run-id> --repo lawkitt/mdoc --name renders-macos-screens
```

**The PC's runners.** Two instances of `scripts/queue-runner.sh` run on the
PC side by side, one per job, each with its own CI checkout and `target/`
on the Dev Drive. They test queue commits only, oldest first, never PR
heads or `main`, and never a group holding a fork or bot PR. A running job
posts `pending` "Running <step> (<elapsed>)" and refreshes it at least
every 4 minutes: the heartbeat `merge-gate` reads. While the PC is off,
`merge-gate` runs either job hosted (Fallback, below).

| | Windows job | Android job |
|---|---|---|
| Home | `E:\ci\queue-windows` | `E:\ci\queue-android` |
| Task (at logon, no window) | `mdoc queue CI windows` | `mdoc queue CI android` |
| Results, 7 days or 20 GB | `E:\ci\out\queue-windows\<sha>\` | `E:\ci\out\queue-android\<sha>\` |
| On the tailnet | `https://windows.<tailnet>.ts.net/ci/queue-windows/` | `…/ci/queue-android/` |

A job's folder holds `log.txt`, `summary.txt` and, for a failure,
`failed-tests.txt`. A failed or timed-out job's `summary.txt` ends with the
log's last 60 lines, without colours. Failing tests are named in the status
("Failed in just test: mdoc-core tests::broken and 1 more"): nextest's from
the log, the Android job's from Gradle's JUnit XML as
`android:<class>#<method>`, the form `.config/quarantine.txt` takes, since
Gradle's log names no emulator test. Android folders also hold the Gradle
reports (`android-reports/`) and that XML (`androidTest-results/`,
`test-results/`). Windows folders keep the headless renders (`renders/`,
about 1.1 GB) only when the job failed, so the 20 GB keep about 7 days of
logs. A runner refuses a job below 40 GB free and says so in a `pending`
status; it deletes its `target/` past 60 GB or when `rust-toolchain.toml`
changes. The Android job builds the core for its emulator's ABI only, x86_64 (arm64-v8a on the Mac)
(`MDOC_ANDROID_ABIS`, which `scripts/android-check.sh` sets when
`MDOC_ANDROID_JOB=always`); `just android-core` and device runs still build
arm64-v8a, and `just mobile --ffi` compile-checks it on macOS. Settings are
the `QUEUE_CI_*` variables in `config()`, set in the home's `runner.env`.

- **Check them** (the fleet preflight): `status.txt` in each home's output
  folder on the tailnet has a `heartbeat:` line from the last 5 minutes. A
  `tailnet: /ci not served` line there (checked every 10 minutes) means the
  page is reachable only on the PC. `queue-runner.sh status` on the PC
  checks at once that `tailscale serve status` lists `/ci` and that the
  runner's `status.txt` answers at its tailnet URL, and fails when either
  doesn't.
- **Install:** on the PC, in PowerShell (not as admin), from a checkout of
  current `main`:
  `powershell -ExecutionPolicy Bypass -File scripts\queue-runner-setup.ps1`.
  It clones the CI checkouts (the first Windows build fills about 30 GB),
  copies the runner, registers the tasks and serves `E:\ci\out` at `/ci`.
  `-Uninstall` removes the tasks and keeps the homes.
- **Update:** an idle runner whose copy of `queue-runner.sh` or `lib.sh`
  differs from `main`'s copies `main`'s and restarts. Re-run the setup to
  update at once.
- **Stop and start:** `Stop-ScheduledTask 'mdoc queue CI windows'`,
  `Start-ScheduledTask …`; re-run the setup to also end a job's build
  processes.
- **After a restart** of the PC, the web task (`mdoc queue CI web`) sets
  `/ci` again when it starts, trying for 5 minutes while Tailscale comes
  up; other served paths are left alone. If `status.txt` still says
  `tailnet: /ci not served`, restart that task, or run
  `tailscale serve --bg --set-path /ci http://127.0.0.1:8787`.

**Fallback.** `merge-gate` (`scripts/merge-gate.sh`, after each run and
status, in follow-up passes, and every 5 minutes) dispatches a PC job hosted when the PC has posted nothing
10 minutes after the queue commit appeared, when its heartbeat is 10
minutes old, when it posts `error`, or at once for a group with a fork or
bot PR: `ci-desktop.yml` with `os=windows` (`ci/windows (hosted)`, the
summary of lint and the four test tiers as two jobs, 20 and 40 minutes),
or `ci-android.yml` (`ci/android (hosted)`, one job, 40 minutes:
`MDOC_ANDROID_JOB=always just android` on `ubuntu-latest` after a udev
rule opens `/dev/kvm`; about 25 minutes cold, with the Gradle reports,
JUnit XML and emulator log uploaded as `android-reports` when it fails).
Both take `-f ref=<sha>` and run on the queue branch. The result that
finished first counts. A lane may dispatch the Android job on its branch:
`gh workflow run ci-android.yml --repo lawkitt/mdoc --ref <branch>`.

**The gate's passes** ([ADR 0100](adr/0100-ci-reliability-after-the-cutover.md)).
`merge-gate.yml`'s job, `merge-gate pass`, posts the `merge-gate` status;
it is not the status itself.

- **GitHub calls.** Each goes through one wrapper (`gh_call`): 30 seconds
  a try, 3 tries with backoff, and a log line naming the endpoint when it
  still fails. HTTP 404, 409 and 422 are answers, not retried. A PR head or
  queue commit skipped because of an error is logged ("skipped: could not
  read …"), and the run ends red with an annotation listing the calls. A
  pass stops reading after 6 minutes (the job has 10), so slow failures
  still leave time to dispatch the follow-up pass.
- **Follow-up pass.** A pass that hit an error, or left a queue commit or a
  running fast tier pending, dispatches `merge-gate-recheck.yml`. It waits
  3 minutes in its own concurrency group (a newer one replaces it) and runs
  `merge-gate.yml` again. The 5-minute schedule stays, but GitHub runs it
  rarely, so nothing relies on it.
- **A job that never ran.** A hosted job cancelled with no runner and no
  step started (GitHub's "not acquired" capacity cancels) is no result. Nor
  is one cancelled after it started (an internal error, a lost runner;
  GitHub reports a test failure as `failure`), unless its annotation says it
  hit its own `timeout-minutes`, nor a summary job whose failing parts are
  all such jobs. Nor is a queue commit's hosted check that is missing once
  the commit's Queue run (found by its head SHA) has completed: its summary
  job was never created (#635), unless one of its parts failed outright or
  timed out, which fails it at once. The gate re-runs the run's failed jobs, at
  most twice a run (by its run attempt), and waits (also while a re-run's
  new job has replaced the failed one, and while the Queue run goes on);
  after that it fails, worded "no macOS runner, 3 attempts", "cancelled on
  macOS, 3 attempts" or "did not run, 3 attempts". A Queue run that
  succeeded without the check has nothing to re-run: "did not run" at once.
  A job that ran and failed, or timed out, still fails at once, and beside
  it nothing is re-run on that commit. A summary's parts are its caller's jobs
  named after it: `ci/detection (2)` for `ci/detection`, `ci/ios-release`
  for `ci/ios`, `ci/macos (tests)` for `ci/macos`, `ci/scripts (bash 3.2)`
  for `ci/scripts`, `fast (unit)` for `fast (ubuntu)`; name new parts that way, or their capacity cancels count
  as failures.
- **Missed enqueue.** A PR that is green, has auto-merge on and is still not
  in the queue 5 minutes later gets a warning in the gate's run (see "A
  stale gate or a missed enqueue" above). The gate repairs nothing.
- **Stale runs.** Runs of queue commits the queue dropped are cancelled; a
  run that finished meanwhile is skipped without a message.
- **Testing.** `merge-gate.yml` always runs `main`'s script, so a change
  to it is proven by `scripts/test-merge-gate.sh` (fakes for every GitHub
  call, and for `gh` itself under the wrapper) and by a shadow run with a
  deliberately broken job, which shows only that nothing regressed.

**`main`.** A push to `main` runs no tests
([ADR 0100](adr/0100-ci-reliability-after-the-cutover.md)): `ci.yml` runs
lint and `just mobile --ffi` (`checks`, on macOS and Windows) and builds
the tests without running them (`test`, and `fast-linux` on Linux), and
`main-caches.yml` builds the iOS core and the detection gate (model set up,
no check). Between them they save every cache the queue's hosted jobs
restore: a branch can restore only its own caches and `main`'s, so only
`main` saves (the setup action's `save-if`, and the model's save step).
rust-cache keys a cache by job id, so each hosted job restores the one
`main`'s job of the same id saves: `ci-desktop.yml`'s `checks` and `test`
jobs are `ci.yml`'s. A job that builds something new needs `main` to build
it under its id, or every queue commit compiles it cold. A cache that
matches its key exactly is never saved again, so new builds reach it only
under a new key: a new job id, or the next change to `Cargo.lock` or the
toolchain. A red `main` stops enqueueing.

**The built iOS core.** Building the core takes 5–6 minutes even with a
cargo cache hit, as rust-cache drops the workspace crates, so `main`'s
build-only `ios` job also saves `target/ios` (the xcframeworks, the Swift
bindings and the licences; #631). `scripts/ios-core-cache.sh key` keys it
on the committed tree of everything the build reads (`crates/`,
`resources/`, the Cargo manifests and lock, the toolchain, `.cargo/`, the
`justfile`, the build script with `lib.sh`, and `XCODE_VERSION`). Both iOS jobs restore it
and on an exact hit skip `just ios-core`, as `just --no-deps` already
skips it for the app recipes; `scripts/ios-build-info.sh` still writes this
commit's `Build.xcconfig`. A miss builds the core as before, then
`scripts/ios-core-cache.sh check` fails if cargo's dep-info names a file the
key does not hash, so a stale core cannot pass. Only `main` saves; the
cache cleanup keeps its newest key.

**CI targets.** `scripts/ci-stats.sh [--days N | --since <UTC time>]` measures ADR 0100's targets from GitHub, read-only, and exits 1 when one fails:
- a queue commit's time to a green `merge-gate`: median 13 minutes or less;
- a PR head's time to `fast (ubuntu)`, from its CI run starting: median 4 minutes or less;
- infrastructure failures ("no macOS runner"): at most 1 a week.

`--since` measures only what ran after a change landed.

**Nightly.** `nightly.yml` runs every hosted job on `main` at 02:17 UTC: the
desktop jobs on both OSes, iOS, Android (`ci-android.yml`), the detection
gate, the quarantined tests and cargo-deny's advisories. A red night opens one issue, "Nightly CI is
red", or comments on the open one; a green night closes it
(`scripts/nightly-issue.sh`). Run it by hand with `gh workflow run
nightly.yml --repo lawkitt/mdoc`.

**cargo-deny.** `lint (ubuntu)` checks licences, bans and sources on every
change, and advisories only when `Cargo.lock` or `deny.toml` changes
(`scripts/deny-ci.sh`): a newly published advisory turns red the PR that
brings the crate in and the nightly run, never every PR. `just deny` still
checks everything locally.

**Shadow branches.** A branch under `ci-shadow/main/` runs exactly as a
queue commit does (`queue.yml`, the PC, `merge-gate`), without merging
anything: for trying a change to CI itself. Its commit subjects end
"(#N)" so the group is not taken for third-party code. Delete it when done.

**Third-party code.** Fork and bot (Dependabot) PRs run the fast tier and,
in `ci.yml`, the full hosted matrix on the PR; their queue groups run
hosted only. Workflows that read PR code have read-only tokens;
`merge-gate.yml` runs `main`'s script, never PR code.

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
  (worktrees of the root checkout) and the queue runners' home `E:\ci`.
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
- **CI** is GitHub's merge queue with hosted jobs and the PC's two runners
  ([CI](#ci)); the Mac runs no CI. Preflight: `status.txt` of
  `/ci/queue-windows/` and `/ci/queue-android/` on the PC has a heartbeat
  from the last 5 minutes. UI items dispatch the hosted desktop job with
  `screens` and download `renders-macos-screens`.
- **Required checks:** `lint (ubuntu)` and `merge-gate` on the PR head
  (the fast tier); the lane waits for them and fixes failures. The hub
  enqueues with `gh pr merge --auto`, never `--admin`, and the
  queue runs the full suite; a dequeued PR goes back to its lane with the
  failing job `merge-gate` names. A lane's gate is the fast subset under
  [Build from source](#build-from-source): `just lint` and the changed
  crates' tests (`wsl` keeps its own gate).
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
