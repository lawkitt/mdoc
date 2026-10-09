# gpui-pdf

mdoc's PDF preview: a virtualized GPUI viewer rendered in pure Rust with
[hayro](https://github.com/LaurenzV/hayro) — no native PDF library. Internal to
the mdoc workspace (`publish = false`).

- Parses once and rasterizes only the visible window of pages, with zoom,
  fit-to-width/page, page navigation and an outline.
- Text extraction (a custom hayro device) drives find-in-PDF.
- Form-field appearances are normalized with `lopdf` before rendering so
  filled values and checkboxes display; the viewer never edits forms.
- PDF link actions reach the OS opener only for `http(s)` URLs.
- Encrypted documents report `is_locked`; the app shows a notice instead.

The highlight pen and area tools are inherited from Zorite and create nothing
in mdoc; see the codebase cleanup interview for their status.

MIT-licensed (see [LICENSE](LICENSE)), derived from Zorite.
