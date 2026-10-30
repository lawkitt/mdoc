# mdoc-markdown

GUI-free Markdown recognition shared by [`mdoc-editor`](../mdoc-editor) and
the mobile apps ([ADR 0085](../../docs/adr/0085-mobile-apps.md)): what counts
as a construct and which source bytes it covers, never how it looks.

- `spans`: `scan` and `scan_line` classify source into spans over byte
  ranges — emphasis, strike, highlight, inline code, links, styled inline
  HTML, bare URLs and the hidden line prefixes — and mark which bytes are
  syntax markers. A span names a theme role (`Color`, `Background`) or a
  colour the document itself gives; each host maps those to its own colours
  and fonts.
- `lines`: one line's ATX heading, thematic break, blockquote, list or task
  prefix.
- `tables`: GFM table regions, cells and alignment.
- `syntax`: the grammar the reader shares with the editor (alerts, table
  styles, writing direction, links, highlight pairing, styled tags).

Offsets are UTF-8 byte offsets into the source, so a span's range is also the
source range other core crates report. The crate depends on no GUI toolkit;
`just lint` checks that gpui stays out of its tree.

MIT-licensed (see [LICENSE](LICENSE)), moved out of `mdoc-editor`, which
derives from Zorite's editor.
