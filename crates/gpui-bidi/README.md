# gpui-bidi

Right-to-left and mixed-direction text support for mdoc's editor. Internal to
the mdoc workspace (`publish = false`).

GPUI shapes a line as one glyph run and assumes byte index and x rise
together, which is false for right-to-left text: carets collapse, clicks land
on the wrong character, and wrapped RTL paragraphs read bottom-to-top.

- `VisualMap` maps logical byte offsets to x positions and back over
  reordered glyphs. Its core is gpui-free and unit-tested on plain
  `(index, x)` pairs.
- `map_of_wrapped` builds a map from a wrapped GPUI line.
- `layout_rows` and `paint_row` break a paragraph in logical order
  (UAX #9) and paint each row.

MIT-licensed (see [LICENSE](LICENSE)), derived from Zorite.
