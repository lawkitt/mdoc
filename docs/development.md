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
| `crates/os-spellcheck` | OS spell-check groundwork, wired only into the editor demo ([roadmap](../ROADMAP.md#deferred): deferred experiment) |

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

Measure runtime hotspots separately from the routine gate:

```sh
cargo test -p mdoc host_performance_matrix -- --ignored --nocapture --test-threads=1
cargo test -p mdoc tabs_host_performance -- --ignored --nocapture --test-threads=1
cargo test -p mdoc-editor search_performance_matrix -- --ignored --nocapture --test-threads=1
```

Use a fixed viewport, representative legal/OCR Markdown, and actual scroll/search
positions; report profile, machine, latency, peak memory, and lifecycle results.
Historical tab measurements found roughly 292 ms long-document redraws in
debug/headless mode despite a 0.007 ms tab activation handler. Reproduce before
optimizing; those timings are not current results or native presentation latency.

Release packaging uses the `mdoc` identity on all platforms. Automatic winget
submission is disabled until `ENABLE_WINGET_PUBLISHING=true` and a `WINGET_TOKEN`
secret are configured in this repository.

To update the app icon, replace `build/appicon.png` with a square PNG (ideally
at least 1024 × 1024), then run `./build/make-icon.sh` on macOS. It requires
the built-in `sips` and `iconutil` tools and Python 3, with no Python packages.
The script preserves the source PNG and regenerates the packaged PNG, Windows
ICO, and macOS ICNS assets.
