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
Open a PDF or DOCX alongside your document to read, zoom, navigate pages, and
search. DOCX preview is read-only and rendered locally with the bundled Rust
converter; the source file is never modified.
Layout is approximate, and extracted text can lose spaces, affecting multiword
search. See [qualification evidence and limitations](docs/docx-viewer-review.md).
PDF form appearances are rendered; this is a viewer, not a PDF form editor.

## Files and shortcuts

Use **New**, **Open**, **Save**, and **Save As** in the toolbar or File menu.
The corresponding shortcuts are Cmd+N/O/S/Shift+S on macOS and
Ctrl+N/O/S/Shift+S on Windows/Linux. Cmd/Ctrl+W closes the window.
Open accepts Markdown (.md, .markdown, .mdown, .txt), PDF, and DOCX files.
**Import as Markdown…** (Cmd/Ctrl+Shift+I) converts PDF, DOC/DOCX, XLS/XLSX,
other supported Office/OpenDocument formats, RTF, EPUB, and CSV into an unsaved
Markdown document. Conversion runs locally in the background, one file at a
time. Save suggests the source name with a `.md` extension; the source is
preserved. Imported PDFs and DOCX files also open in the side pane. Other
supported import formats remain text-only and show that source preview is
unavailable.

Import retains text and structure, not embedded images. On Apple Silicon macOS,
**Set up OCR** in the main bar downloads about 54 MB of verified components for
printed English and Russian. Setup is also offered when importing a PDF that
needs recognition. Once ready, scanned pages are recognized locally and offline;
document contents are never uploaded. The original PDF remains unchanged.
Low-confidence or incomplete pages produce a persistent review warning outside
the Markdown. Skipping setup imports usable native text with omitted-page
warnings; if no usable text is available, the current document is preserved.
Other platforms currently support native-text import only. Handwriting and
complex table reconstruction are not qualified. See [OCR qualification and
limitations](docs/local-ocr-qualification.md).

You can keep editing during conversion. Switching Markdown documents discards its
pending result, and successful conversion prompts before replacing unsaved edits.

Click local Markdown, PDF, or DOCX links to open them; web links open in your
browser. **Close Preview** returns to a full-width editor without changing your
document.
Use the **☀ Light / ☾ Dark** toolbar button to switch the editor and PDF pane
between the two themes. The app starts in dark mode; the toggle lasts for the session.

Documents are ordinary UTF-8 files. Saves use atomic replacement, and an external
change to the current file is reported instead of silently overwritten. New,
Open, and Close prompt to save or discard unsaved edits; cancelling Save As
also cancels the pending operation.

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

The app shell lives in `src/main.rs`; file persistence in `src/document.rs`.
`src/import.rs` isolates the pinned AnyDoc and pdf-inspector forks; `src/ocr.rs`
owns explicit OCR setup and offline runtime paths. See
[the integration and upstream-update notes](docs/anydoc-integration.md).
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
