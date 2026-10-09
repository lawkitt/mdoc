# Changelog

All notable changes to **mdoc** are documented here.

## Unreleased

- Empty page: a blank Markdown tab shows a centered **Open files…** button,
  supported formats, the multi-file hint and ⌘O, and still accepts typing.
  Files dragged from the OS open when dropped on the empty page or on the
  sidebar, at an insertion line between documents; dragging over the collapsed
  rail reveals the list (ADR 0028).

- Chrome-style document sidebar: expanded by default with a + row under the
  last document. When collapsed, hovering or keyboard focus slides the full
  list out over the editor; it slides back shortly after the pointer leaves.
  Compact entries switch documents directly, replacing the filename popup
  (ADR 0027).

- First-use model setup asks in place: a Markdown-pane card for OCR and a
  Replacements-panel card for Pseudonymize show the download size, total-byte
  progress, Cancel and Retry, then continue the task. Settings no longer opens
  by itself; model choices save immediately with plain-language rows.
  English OCR (PP-OCRv6 Small) is the new default (ADR 0026).

- Multi-file opening with lazy retained tabs, paths-only session restoration,
  compact sidebar navigation, and automatic PDF/DOCX conversion when OCR is
  unnecessary; explicit inline OCR choices preserve the original source.
- Slim Markdown and PDF/DOCX overflow scrollbars, horizontal preview scrolling,
  and quieter expandable conversion notices.
- Product direction, roadmap, domain glossary, and consolidated architecture
  decisions; manual build-cache cleanup instructions.
- Read-only side-by-side DOCX preview with a local Rust converter, plus a
  read-only DOCX comment list that never enters editor text.
- Literal Markdown find (Cmd/Ctrl+F, Cmd/Ctrl+G navigation, Match case
  toggle) over rendered visible text.
- Local offline OCR import for scanned PDFs on Apple Silicon macOS and
  Windows x64, with explicit setup and review warnings.
- Search-bar styling pass and Markdown search performance coverage.

## 0.1.0

- Initial standalone mdoc repository, derived from Zorite.
- File-based Markdown WYSIWYG editing with atomic saves and external-change detection.
- Side-by-side PDF viewing and local document import through AnyDoc.
- Light and dark themes, with native file actions on macOS, Windows, and Linux.

For the original project's release history, see
[Zorite releases](https://github.com/packetThrower/zorite/releases).
