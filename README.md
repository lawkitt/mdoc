# mdoc

A local document-preparation utility for lawyers: convert PDF (including scans),
Word, Excel, PowerPoint, OpenDocument, RTF and other popular formats into
editable Markdown for AI tools such as ChatGPT or Claude. Local OCR, source
previews and WYSIWYG editing support review before handoff.

Local pseudonymization is a core feature: mdoc proposes consistent replacements
for names, organizations, contact details and identifiers, and the lawyer reviews
and approves every one before copying. It runs on pinned local GLiNER2 FP16/FP32
bundles. It is review assistance, not guaranteed anonymization: the
[qualification results](tests/fixtures/pseudonymization/README.md) record known
English/Russian and hidden-source misses.

Built with Rust and GPUI. Full features (OCR and automatic pseudonymization) run
on Apple Silicon macOS and Windows x64; Intel macOS, Windows ARM64 and Linux
builds import native text only. Marketing site: https://lawkitt.com/tools/mdoc/

See [ROADMAP.md](ROADMAP.md) for planned features and priorities,
[CONTEXT.md](CONTEXT.md) for domain terms, and [ADRs](docs/adr/) for durable decisions.

All dependencies, including the pinned `lawkitt/anydoc` converter, are public;
no Git credentials are needed to build.

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
Existing `==highlights==`, colored `<mark>` backgrounds, and `<span>` text colors
also render. Saving and Copy Markdown retain their original source syntax.
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

Use the **Open**, **Save**, **Save As**, and anonymous-person icons alongside
**Copy Markdown** in the compact toolbar. The anonymous-person icon is
**Pseudonymize**: it scans once, then shows or hides the review. **Settings** and the sun/moon theme
control are directly accessible icons with tooltips. **New** is the sidebar plus
button. Toolbar groups wrap when space is limited; commands remain visible.
The sidebar is expanded by default when no choice is saved and remembers an
explicit expand/collapse choice. **+** is a full-width row under the last
document. Compact rail entries activate their document directly; hovering the
collapsed rail (~200ms) or focusing it with the keyboard slides the full list
out over the editor, closing ~300ms after the pointer leaves (Escape dismisses
it). The File menu retains file commands.
The corresponding shortcuts are Cmd+N/O/S/Shift+S on macOS and
Ctrl+N/O/S/Shift+S on Windows/Linux. Cmd/Ctrl+W closes the active tab;
Cmd/Ctrl+Q quits. Ctrl+Tab and Ctrl+Shift+Tab switch tabs.

**Copy Markdown** in the toolbar or File menu copies the complete current
Markdown source, including unsaved edits, regardless of selection. It adds no
wrapper or metadata. Brief **Copied** feedback confirms the handoff; conversion
warnings remain visible below the toolbar for review.

**Settings** (Cmd/Ctrl+comma) opens application defaults, including with no
document open. Text recognition offers **English** (PP-OCRv6 Small, recommended)
and **English & Russian** (Cyrillic bundle); pseudonymization offers **Standard**
(GLiNER2 FP16, recommended) and **Full precision** (FP32). Choices save
immediately. Each row shows one status; a selected model that is not downloaded
reads "Downloads on first use". Selection never downloads a model: use
**Download** explicitly, or approve the download where the feature is first used. **Details** contains **Repair**
and **Remove model**, along with evidence and technical information. Shared
runtimes are retained and model files cannot be removed during model work.
**Details** includes model description, languages/category coverage, license,
revision, runtime/storage information, a Hugging Face model-card link and pinned
download files. Russian detection remains exploratory, with known misses.
Language claims do not establish accuracy on your files.

**Advanced** exposes OCR resolution (150/200/300 DPI), minimum recognition
confidence (0–1), and detection threshold (0–1). Explicit OCR recognizes every
selected page using fixed Force routing. Numeric fields are saved with **Save**;
**Done/Escape** discards unsaved numbers. **Reset defaults** saves the
recommended models, 150 DPI, confidence 0 and threshold 0.5. Saved choices of
other models are kept when defaults change. Existing results keep
their captured configuration. Setup continues after the dialog closes;
cancellation retains completed verified artifacts. Quitting waits for active
model work before releasing the native runtimes.
Saved preferences selecting a removed model require an explicit reset or
supported selection in Settings; the app does not silently substitute a model.

Comparison commands under **Advanced** in Settings open an isolated read-only window.
It captures the original PDF or the complete current Markdown once. Runs execute
sequentially on that snapshot, with an optional OCR page range frozen on the
first run. **New comparison** captures fresh input. Recognition output is
separate from Prepared Markdown; detected spans include source byte offsets and
confidence. Counts and timing are measurements, not accuracy scores. Overrides
never change defaults or the editor. **Export report** writes JSON explicitly,
without replacement mappings; OCR Markdown can be saved separately. Exports
protect the document/source paths and their aliases. Comparisons stay in memory
until close, and cancellation holds the model slot until the native call stops.
See [model settings verification](tests/fixtures/model-settings/README.md).

**Pseudonymize** (anonymous-person icon or Cmd/Ctrl+Shift+P) explicitly scans the complete current
Markdown source in the background, including link destinations, image paths,
code and HTML. Automatic scanning is experimental on Apple Silicon macOS and
Windows x64; manual review remains available on every platform. When the
selected model is missing, the Replacements panel shows a setup card with the
download size (Standard: up to 709 MB including the native runtime if absent;
Full precision: up to 1,323 MB) and **Download & scan**, **Cancel** and
**Choose model…**; no scan starts until the model is installed. Progress counts
total bytes, and the scan starts once setup succeeds. Setup verifies pinned
sizes and SHA-256 digests; subsequent scans run offline. Ordinary opening and editing need no model.
Scans use bounded 512-token schema/text windows with overlapping context,
a 2 MiB source limit and a two-minute cooperative deadline. Comparison reports
identify the model precision, engine and captured configuration.

A scan only proposes replacements; no Markdown changes until **Apply
replacements**, which applies every pending proposal as one undo step. Failed,
partial, stale or cancelled scans propose nothing. Candidates have subtle
highlights; hidden source receives a marker beside its containing element.
Clicking a highlight, or Alt+Enter at its caret, opens the **Replacements** panel
together with a popup at that exact word. Alt+Up/Down navigate highlights.
The popup edits the alias for this mention, the same wording or the entire
entity, links mentions to an existing entity, corrects the category, records
ownership and offers scoped Keep/Restore. Applied replacements stay highlighted
and restorable while their originals live in the open document. Aliases start
with a letter, end with a letter/number and use ASCII letters, numbers,
underscores and hyphens to remain safe inside Markdown, URLs and HTML. Exact
repeats exclude substrings inside longer words; initials and inflected variants
stay separate until linked by the lawyer. **Copy Markdown** remains a separate
step that copies the exact current source. Check the remaining text before
copying: the experimental detector can miss PII and identifying context remains.
Original files, filenames, attachments and undo history are retained; this
feature prepares Markdown for handoff and does not erase local source data.
Dates, amounts and other identifying context receive no automatic generalization.

Use the panel's **⋯** menu to choose a selection category, then **Add selected text**
(Cmd/Ctrl+Alt+P) for missed spans or information you choose to replace manually.
Selections crossing Markdown delimiters, line breaks or more than 1,024 bytes
must be narrowed. Find and review highlights coexist. Edits revalidate candidates
and cancel a pending scan; undo restores accepted text and makes it reviewable
again. **Rescan** retains mappings and exclusions. **Close review** hides highlights and
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
appends tabs in selection order, and activates the first selected file. An empty
Markdown tab shows a centered **Open files…** button with the supported formats;
typing starts writing instead. Files dragged from Finder or Explorer open when
dropped on that empty page, or on the sidebar at the accent insertion line
(dragging over the collapsed rail slides the list out). Reopening
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
and Windows x64, a PDF that needs recognition shows a card in the Markdown pane
listing the affected pages, with **Download & recognize** (or **Run OCR** once
installed), **Use native text only** and **Choose model…**. Its footnote states
the one-time download (English OCR: about 67 MB on macOS including runtimes).
Download progress, cancellation and failures with **Retry** appear in the same
card, and recognition continues automatically after setup. Recognition
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
browser. **Hide original** returns to a full-width editor without changing your
document. **Show original** reopens the same attachment, including for an unsaved
Markdown tab. Slim draggable scrollbars appear when content overflows. PDF/DOCX
previews initially fit their pane; after zooming, use the trackpad, Shift+wheel,
or the horizontal scrollbar to move sideways. Conversion warnings and errors
use compact notices with expandable details and dismissal controls.
Drag the pane divider to adjust the split; the ratio and visibility stay with
the document and saved session. Small windows offer a **Markdown / Original**
switch. The Original header identifies the retained source file; DOCX rendering
remains approximate. PDF navigation, zoom, fit, find, and markup tools remain
directly visible and wrap in narrow panes.
Tables without saved widths wrap to fit readable columns. Explicit widths remain
unchanged; wider tables have a draggable horizontal scrollbar.
Use the toolbar's sun/moon icon to switch the editor and PDF pane
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
| `crates/os-spellcheck` | OS spell-check groundwork, wired only into the editor demo (ROADMAP: deferred experiment) |

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

To update the app icon, replace `build/appicon.png` with a square PNG (ideally
at least 1024 × 1024), then run `./build/make-icon.sh` on macOS. It requires
the built-in `sips` and `iconutil` tools and Python 3, with no Python packages.
The script preserves the source PNG and regenerates the packaged PNG, Windows
ICO, and macOS ICNS assets.

## License and attribution

mdoc is derived from [Zorite](https://github.com/packetThrower/zorite).
The app remains [GPL-3.0-or-later](LICENSE); reusable crates retain their MIT
licenses and original copyright notices. See [NOTICE.md](NOTICE.md) for
provenance and [THIRD-PARTY-LICENSES.html](THIRD-PARTY-LICENSES.html) for dependency
licenses.
