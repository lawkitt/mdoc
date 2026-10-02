# mdoc

A local document-preparation utility for lawyers: convert PDF, DOCX, and other
popular formats into editable Markdown for AI agents in other tools. Local OCR,
source previews, and WYSIWYG editing support review before handoff.
Built with Rust and GPUI for macOS, Windows, and Linux.

See [ROADMAP.md](ROADMAP.md) for planned features and priorities,
[CONTEXT.md](CONTEXT.md) for domain terms, and [ADRs](docs/adr/) for durable decisions.
Inline pseudonymization is available experimentally using the pinned GLiNER2
model. The [initial qualification](tests/fixtures/pseudonymization/README.md)
found significant English/Russian and hidden-source misses; experimental use
was explicitly authorized despite those blockers. This is review assistance,
not guaranteed anonymization.

Building requires access to the pinned private `lawkitt/anydoc` dependency.
Authenticate Git with an account that has access before running Cargo.

## Run

```sh
git clone https://github.com/lawkitt/mdoc.git
cd mdoc
```

```sh
cargo run
cargo run -- path/to/note.md
cargo run -- path/to/reference.pdf
cargo run -- path/to/reference.docx
```

Write Markdown directly: headings, emphasis, lists, checkboxes, tables, links,
quotes, and code blocks render as you edit. Syntax is revealed near the caret.
Local images resolve relative to the Markdown file and load in the background.
Open a PDF or DOCX in its own preview tab to read, zoom, navigate pages, and
search. DOCX preview is read-only and rendered locally with the bundled Rust
converter; the source file is never modified.
Layout is approximate, and extracted preview text can lose spaces, affecting
multiword search. DOCX comments appear in a read-only side list and never enter
the Markdown. Preview rejects encrypted, macro-bearing, and tracked-change files;
it caps input at 50 MiB, expanded content at 250 MiB, and its worker at 30 seconds.
These preview limits are separate from Markdown conversion support.
PDF form appearances are rendered; this is a viewer, not a PDF form editor.

## Files and shortcuts

Use **New**, **Open**, **Save**, and **Save As** in the toolbar or File menu.
The corresponding shortcuts are Cmd+N/O/S/Shift+S on macOS and
Ctrl+N/O/S/Shift+S on Windows/Linux. Cmd/Ctrl+W closes the active tab;
Cmd/Ctrl+Q quits. Ctrl+Tab and Ctrl+Shift+Tab switch tabs.

**Copy Markdown** in the toolbar or File menu copies the complete current
Markdown source, including unsaved edits, regardless of selection. It adds no
wrapper or metadata. Brief **Copied** feedback confirms the handoff; conversion
warnings remain visible below the toolbar for review.

**Pseudonymize** (Cmd/Ctrl+Shift+P) explicitly scans the complete current
Markdown source in the background, including link destinations, image paths,
code and HTML. Automatic scanning is experimental on Apple Silicon macOS and
Windows x64; manual review remains available on every platform. The first use
offers a separate **Download experimental model** action (up to 709 MB including
the native runtime if absent). Setup verifies pinned sizes and SHA-256 digests;
subsequent scans run offline. Ordinary opening and editing need no model.

Candidates have subtle highlights; hidden source receives a marker beside its
containing element. Click a candidate, or use Alt+Enter at its caret, to open one
review popup. Enter accepts, Alt+K keeps, and Escape closes it.
Alt+Up/Down and **Previous / Next** navigate candidates. Shift-click
bypasses review to edit normally; hover strengthens the highlight without opening
a popup. The popup shows original text, its source fragment, a stable placeholder
and occurrence count. **Accept** replaces all exact occurrences as one undo step;
**Keep** changes no text. Both offer an explicit single-occurrence option. Edit
the replacement field or explicitly link a variant to an existing placeholder;
tokens start with a letter, end with a letter/number and use ASCII letters,
numbers, underscores and hyphens to remain safe inside
Markdown, URLs and HTML. Exact repeats exclude substrings inside longer words;
initials and inflected variants stay separate until linked by the lawyer.

Use **Selection type** to choose a category, then **Add selection**
(Cmd/Ctrl+Alt+P) for missed spans or information you choose to replace manually.
Selections crossing Markdown delimiters, line breaks or more than 1,024 bytes
must be narrowed. Find and review highlights coexist. Edits revalidate candidates
and cancel a pending scan; undo restores accepted text and makes it reviewable
again. **Rescan** retains mappings and exclusions. **Done** hides highlights and
retains edits. **Cancel** rejects pending results without undoing accepted edits;
an in-progress bounded native call finishes before releasing the inference slot.
Only one scan runs at a time, engines are dropped after each scan, and oversized
inputs or elapsed deadlines produce an error rather than accepting partial scans
(2 MiB source, 512 actual schema/text tokens per adaptive window, two-minute
cooperative deadline). These limits are experimental safeguards, not accuracy or
responsiveness guarantees.

Mappings and review decisions stay only in the live document, survive tab switches,
and disappear on tab close; restart/save/copy never export them. Save and copy
still transfer only Markdown. The remaining count does not establish that all
identifying information was detected. See [inline verification](tests/fixtures/pseudonymization/inline-review.md)
for measured checks and outstanding native/platform acceptance.

The collapsible left sidebar lists open files. Tabs and **+** remain visible in
its compact rail, with filename tooltips and status indicators. Use **+** for an
empty Markdown tab, drag tabs to reorder them, and **×** or the compact tab's
context menu to close one. **Open…** accepts one or more files (no folders),
appends tabs in selection order, and activates the first selected file. Reopening
a file selects its existing tab and preserves edits. Other tabs load when
selected. Each tab keeps its undo history, selection, search, and reading
positions; unsupported files are summarized and load failures can be retried.
Opening files removes only the automatic untouched blank placeholder. Creating
another blank intentionally preserves both blanks, including across restart.

Markdown (.md, .markdown, .mdown, .txt) opens for editing. PDF and DOCX open with
the original preview on the right and convert to Markdown automatically on first
selection. A PDF with any pages requiring OCR waits for an explicit inline
choice; incomplete native text is never silently imported. Other supported
Office/OpenDocument formats, RTF, EPUB, and CSV retain **Convert to Markdown**
(Cmd/Ctrl+Shift+I). Conversion runs locally, one file at a time, in the source's
tab. Save suggests the source name with a `.md` extension and preserves the
original. An untouched automatic conversion closes without a save prompt;
editing enables normal unsaved-change protection. Converted tabs cannot be
converted again over subsequent edits.

Conversion retains text and structure, not embedded images. On Apple Silicon macOS
and Windows x64, **Set up OCR** in the main bar downloads verified components
(about 54 MB on macOS, 99 MB on Windows) for
printed English and Russian. Setup is also offered when converting a PDF that
needs recognition. The Markdown pane highlights affected pages and offers
**Run OCR** or **Set up OCR**, plus **Extract native text only**. Recognition
requires an explicit choice even when OCR is ready; you can keep reading the
preview without converting. Recognition runs locally and offline;
document contents are never uploaded. The original PDF remains unchanged.
Low-confidence or incomplete pages produce a persistent review warning outside
the Markdown. Skipping setup converts usable native text with omitted-page
warnings; if no usable text is available, the current document is preserved.
Image-only DOCX content currently has no app OCR path; DOCX conversion skips OCR.
Windows OCR requires the [Microsoft Visual C++ Redistributable (x64)](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
If its runtime cannot load, setup provides installation and retry instructions.
Windows ARM64, Linux, and Intel macOS currently support native-text import only. Handwriting and
complex table reconstruction are not qualified. See [OCR qualification and
limitations](tests/fixtures/ocr-qualification/README.md).

You can switch tabs while conversion runs; the result stays in its source tab
without stealing focus. Sidebar labels show importing, queued, and loading states.
Closing a tab cancels its pending work. Conversion cancellation is cooperative between
conversion stages; an in-progress library call finishes before releasing its slot.
DOCX preview conversions run one at a time.
Multi-file opening retains this lazy, bounded workflow; it does not eagerly
convert and save the whole selection. Bulk-conversion redesign is deferred.

Cmd/Ctrl+F opens Markdown find, Cmd/Ctrl+G moves to the next occurrence, and
Shift+Cmd/Ctrl+G moves to the previous one. Match case is optional. Find searches
rendered visible text; PDF/DOCX search stays in its preview pane.

Click local file links to open them; web links open in your
browser. **Close Preview** returns to a full-width editor without changing your
document. **Show Preview** reopens the same attachment, including for an unsaved
Markdown tab. Slim draggable scrollbars appear when content overflows. PDF/DOCX
previews initially fit their pane; after zooming, use the trackpad, Shift+wheel,
or the horizontal scrollbar to move sideways. Conversion warnings and errors
use compact notices with expandable details and dismissal controls.
Use the **☀ Light / ☾ Dark** toolbar button to switch the editor and PDF pane
between the two themes. The app starts in dark mode; the toggle lasts for the session.

Documents are ordinary UTF-8 files. Saves use atomic replacement, and an external
change to the current file is reported instead of silently overwritten. Closing
a dirty tab or quitting prompts to save or discard edits; cancelling Save As
also cancels the pending operation. Save As cannot overwrite a file open in
another tab.

Saved Markdown tabs, unconverted source tabs, and reading positions reopen after restarting.
Unconverted source tabs close without a save prompt. A small
`mdoc/session.json` file in the platform's local application-data directory stores
paths and view metadata only. Unsaved text and undo history are not restored;
untitled tabs must be saved before quitting to retain their pairing. Restored
content loads when first selected; hidden previews load when shown.

Documents remain ordinary local files; there is no notebook database or account.
mdoc does not access or migrate old Zorite notebooks.
There is no separate reader/raw mode, Markdown-to-PDF export, math/diagram
engine, or remote-image fetching. Password-protected PDFs are not supported.

## Development

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Keep build caches for fast warm rebuilds. For occasional cleanup from the
repository root, use `rtk cargo clean --workspace` to remove workspace artifacts
while retaining dependency artifacts. Use `rtk cargo clean` for a full reset;
the next build will be cold. Cleanup is manual; no automatic size limit applies.
See [Cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html).

Keep the current app/reusable-crate layout. Extract ownership only for a concrete
problem; conversion and OCR algorithm changes belong in the dependency forks.

| Location | Responsibility |
| --- | --- |
| `src/main.rs`, `src/tabs.rs` | Document views, native actions, tab identity, admission, scheduling, and chrome |
| `src/document.rs`, `src/document_session.rs` | Atomic saves, external-change checks, accepted document identity and provenance |
| `src/session_store.rs` | Paths and view metadata restoration; no document text |
| `src/import.rs`, `src/import_session.rs`, `src/ocr.rs` | Library integration, pending jobs, explicit verified setup and offline OCR |
| `src/preview.rs`, `src/docx_preview.rs` | Preview ownership and supervised local DOCX worker |
| `src/docx_comments.rs`, `src/comment_panel.rs` | Read-only DOCX comments |
| `src/markdown_search.rs`, `src/search_session.rs` | Markdown find controls and revision-aware search scheduling |
| `src/pseudonymization.rs`, `src/pseudonymization_detector.rs`, `src/pseudonymization_ui.rs` | In-memory review/mapping policy, experimental offline detection/setup, inline controls |
| `src/images.rs`, `src/style.rs` | Document-relative local images and app styling |
| `src/ui_tests.rs`, `src/tabs_tests.rs`, `src/perf_tests.rs` | Headless flows, ownership/lifetime checks, and opt-in performance measurements |
| `crates/mdoc-editor`, `crates/mdoc-markdown` | Host-agnostic WYSIWYG, rendered-text search, and Markdown recognition |
| `crates/gpui-pdf`, `crates/gpui-bidi` | Virtualized PDF preview and bidirectional text layout |
| `crates/os-spellcheck` | Standalone editor demo dependency only |

Cargo.toml and Cargo.lock define dependency revisions; `src/ocr.rs` defines
runtime URLs/digests, and the pinned pdf-inspector manifest defines OCR model
artifacts. Avoid copying these into another configuration. AnyDoc supplies
non-PDF conversion; pdf-inspector handles PDFs directly.

For fork updates, test the fork first, review lockfile/transitive changes, then
run the full gate above and compare outputs against the source before changing
fixture expectations. AnyDoc's PR 153 port is attributed in its fork; the fork
began as a snapshot with unrelated upstream history, so port reviewed upstream
changes rather than assuming a normal merge. Its transitive pdf-inspector 1.14.2
baseline is distinct from the app's direct OCR-capable fork. If Cargo's Git fetch
cannot authenticate, use `CARGO_NET_GIT_FETCH_WITH_CLI=true` with existing Git
credentials; do not change global Cargo configuration.

Use [import](tests/fixtures/import/README.md),
[DOCX](tests/fixtures/docx-preview/README.md), and
[OCR](tests/fixtures/ocr-qualification/README.md) fixture notes for reproduction
and qualification limits. Native appearance, shortcuts, IME, GPU presentation,
and execution on other platforms require native checks; headless tests do not
establish them. Do not distribute local user-provided fixtures.

Measure runtime hotspots separately from the routine gate:

```sh
rtk cargo test -p mdoc host_performance_matrix -- --ignored --nocapture --test-threads=1
rtk cargo test -p mdoc tabs_host_performance -- --ignored --nocapture --test-threads=1
rtk cargo test -p mdoc-editor search_performance_matrix -- --ignored --nocapture --test-threads=1
```

Use a fixed viewport, representative legal/OCR Markdown, and actual scroll/search
positions; report profile, machine, latency, peak memory, and lifecycle results.
Historical tab measurements found roughly 292 ms long-document redraws in
debug/headless mode despite a 0.007 ms tab activation handler. Reproduce before
optimizing; those timings are not current results or native presentation latency.

Release packaging uses the `mdoc` identity on all platforms. Automatic winget
submission is disabled until `ENABLE_WINGET_PUBLISHING=true` and a `WINGET_TOKEN`
secret are configured in this repository.

## License and attribution

mdoc is derived from [Zorite](https://github.com/packetThrower/zorite).
The app remains [GPL-3.0-or-later](LICENSE); reusable crates retain their MIT
licenses and original copyright notices. See [NOTICE.md](NOTICE.md) for
provenance and [THIRD-PARTY-LICENSES.html](THIRD-PARTY-LICENSES.html) for dependency
licenses.
