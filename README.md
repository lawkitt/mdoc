# mdoc

A lightweight desktop Markdown WYSIWYG editor with side-by-side PDF and DOCX preview.
Built with Rust and GPUI for macOS, Windows, and Linux.

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
Layout is approximate, and extracted text can lose spaces, affecting multiword
search. See [qualification evidence and limitations](docs/docx-viewer-review.md).
PDF form appearances are rendered; this is a viewer, not a PDF form editor.

## Files and shortcuts

Use **New**, **Open**, **Save**, and **Save As** in the toolbar or File menu.
The corresponding shortcuts are Cmd+N/O/S/Shift+S on macOS and
Ctrl+N/O/S/Shift+S on Windows/Linux. Cmd/Ctrl+W closes the active tab;
Cmd/Ctrl+Q quits. Ctrl+Tab and Ctrl+Shift+Tab switch tabs.

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
Windows OCR requires the [Microsoft Visual C++ Redistributable (x64)](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
If its runtime cannot load, setup provides installation and retry instructions.
Windows ARM64, Linux, and Intel macOS currently support native-text import only. Handwriting and
complex table reconstruction are not qualified. See [OCR qualification and
limitations](docs/local-ocr-qualification.md).

You can switch tabs while conversion runs; the result stays in its source tab
without stealing focus. Sidebar labels show importing, queued, and loading states.
Closing a tab cancels its pending work. Conversion cancellation is cooperative between
conversion stages; an in-progress library call finishes before releasing its slot.
DOCX preview conversions run one at a time.

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

This fork removes journals, notebooks, SQLite/encryption, graph views,
whiteboards, notebook importers, settings/theme packs, localization, and update checks.
It does not access or migrate an existing Zorite notebook. Export any notes you
need from the original app as Markdown before opening them here.
There is no separate reader/raw mode, Markdown-to-PDF export, math/diagram
engine, or remote-image fetching. Password-protected PDFs are not supported.

## Development

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The tab/sidebar owner lives in `src/tabs.rs`, with paths-only restoration in
`src/session_store.rs`. Each tab retains the document view in `src/main.rs`;
file persistence lives in `src/document.rs`. See
[tab decisions and measured validation](docs/sidebar-tabs-decisions.md).
DOCX preview and comments live in `src/docx_preview.rs`,
`src/docx_comments.rs`, and `src/comment_panel.rs`; Markdown find lives in
`src/markdown_search.rs` over the `mdoc-editor` search index.
`src/import.rs` isolates the pinned AnyDoc and pdf-inspector forks; `src/ocr.rs`
owns explicit OCR setup and offline runtime paths. See
[the integration and upstream-update notes](docs/anydoc-integration.md),
[Markdown search decisions](docs/markdown-search-decisions.md), and
[local OCR decisions](docs/local-ocr-decisions.md).
`mdoc-editor` supplies WYSIWYG, `gpui-pdf` supplies PDF rendering, and
`gpui-bidi` supplies bidirectional text. `mdoc-markdown` remains because the
editor uses its Markdown recognition helpers; it is not a separate app view.
`os-spellcheck` is retained only for the standalone editor demo.

Release packaging uses the `mdoc` identity on all platforms. Automatic winget
submission is disabled until `ENABLE_WINGET_PUBLISHING=true` and a `WINGET_TOKEN`
secret are configured in this repository.

## License and attribution

mdoc is derived from [Zorite](https://github.com/packetThrower/zorite).
The app remains [GPL-3.0-or-later](LICENSE); reusable crates retain their MIT
licenses and original copyright notices. See [NOTICE.md](NOTICE.md) for
provenance and [THIRD-PARTY-LICENSES.html](THIRD-PARTY-LICENSES.html) for dependency
licenses.
