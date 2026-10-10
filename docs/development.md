# Development notes

Run the [repository gate](../README.md#development) before delivering code changes.

Keep build caches for fast warm rebuilds. For occasional cleanup from the
repository root, use `cargo clean --workspace` to remove workspace artifacts
while retaining dependency artifacts. Use `cargo clean` for a full reset;
the next build will be cold. Cleanup is manual; no automatic size limit applies.
See [Cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html).

Keep the current app/reusable-crate layout. Extract ownership only for a concrete
problem; conversion and OCR algorithm changes belong in the dependency forks.

| Location | Responsibility |
| --- | --- |
| `src/main.rs` | Application startup, menus, key bindings and window creation |
| `src/workspace.rs` | The window shell: document sidebar, tab identity, admission, scheduling and session persistence |
| `src/document_view.rs`, `src/document_view/chrome.rs` | One tab's document view: editor, source preview, import/OCR state, actions and toolbar chrome |
| `src/document.rs`, `src/document_session.rs` | Atomic saves, external-change checks, accepted document identity and provenance |
| `src/session_store.rs` | Paths and view metadata restoration; no document text |
| `src/import.rs`, `src/import_session.rs`, `src/ocr.rs` | Library integration, pending jobs, explicit verified setup and offline OCR |
| `src/preview.rs`, `src/docx_preview.rs` | Preview ownership and supervised local DOCX worker |
| `src/docx_comments.rs`, `src/comment_panel.rs` | Read-only DOCX comments |
| `src/markdown_search.rs`, `src/search_session.rs` | Markdown find controls and revision-aware search scheduling |
| `src/pii.rs`, `src/pii/detector*`, `src/pii/ui*` | Pseudonymization: offline detection/setup, scan/edit controller, Replacements panel and word popup, UI regression tests |
| `src/images.rs`, `src/style.rs` | Document-relative local images and app styling |
| `src/document_view_tests.rs`, `src/workspace_tests.rs`, `src/perf_tests.rs` | Headless flows, ownership/lifetime checks, and opt-in performance measurements |
| `crates/mdoc-editor` | Host-agnostic WYSIWYG, rendered-text search, and Markdown recognition |
| `crates/mdoc-pii`, `crates/mdoc-history` | GUI-free pseudonymization review model and the plain edit transactions it follows |
| `crates/gpui-pdf`, `crates/gpui-bidi` | Virtualized PDF preview and bidirectional text layout |
| `src/spelling.rs`, `crates/mdoc-spell` | Spellcheck: background checks, toolbar indicator and navigation; GUI-free EN/RU engine with bundled dictionaries ([ADR 0036](adr/0036-bundled-spellcheck.md)) |

Cargo.toml and Cargo.lock define dependency revisions; `src/ocr.rs` defines
runtime URLs/digests, and the pinned pdf-inspector manifest defines OCR model
artifacts. Avoid copying these into another configuration. AnyDoc supplies
non-PDF conversion; pdf-inspector handles PDFs directly.

For fork updates, test the fork first, review lockfile/transitive changes, then
run the [repository gate](../README.md#development) and compare outputs against
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
| `just check` | `fmt-check`, `deny`, `lint`, `test` — the CI gate |
| `just smoke-ocr` | Downloads the pinned OCR runtime and recognizes the EN/RU fixtures (release gate) |
| `just smoke-models [--download]` | Default OCR and pseudonymization models through setup and offline paths |
| `just check-all` | `check` + both smoke recipes |
| `just perf` | Ignored performance tests, serially |
| `just package` | This machine's installers into `dist/` |
| `just release <version>` | Release commit and tag (below) |
| `just icon`, `just actionlint` | Icon assets; workflow lint |

Heavy, download and machine-dependent tests run only through these recipes, on
the Apple Silicon Mac and the Windows x64 PC.

## Releasing

Releases ship only for Apple Silicon macOS (`.dmg`) and Windows x64 (NSIS
`.exe`; `.msi` for stable versions). The repository version is the source of
truth: `just release 0.1.0-beta.1` checks that `main` is clean and current, runs
the checks (optionally `check-all`), sets the version in `Cargo.toml`,
`Cargo.lock` and `Info.plist`, names the CHANGELOG section for stable versions,
commits `release: v…`, tags, and pushes after confirmation. The tag runs
`.github/workflows/release.yml`: version check, CI, OCR smoke on both targets,
packaging, then a draft GitHub Release with `SHA256SUMS` and build provenance.
Review the draft and publish it by hand. "Run workflow" on the Release workflow
builds an unpublished test build from any branch.

## App icon

To update the app icon, replace `build/appicon.png` with a square PNG (ideally
at least 1024 × 1024), then run `just icon` on macOS. It requires
the built-in `sips` and `iconutil` tools and Python 3, with no Python packages.
The script preserves the source PNG and regenerates the packaged PNG, Windows
ICO, and macOS ICNS assets.
