# Synthetic PDF Source fixture

`comments.pdf` is a two-page agreement written by `generate.py`, which uses
only Python's standard library and no compression, so every platform writes
the same bytes. It contains no private document.

Page 1 has an offset CropBox `[36 36 576 756]` and holds a sticky note with a
popup, a reply and a `Completed` review state; a highlight with a comment, a
popup and a reply; a FreeText box with text drawn by its appearance; an
underline, a strikeout and a squiggly with only `/RC`; and, drawn as part of
the page, a highlight without a comment, a stamp, ink and a FreeText that is
only an appearance. Page 2 has `/Rotate 90` and text drawn to read upright,
with a commented highlight and a sticky note. `generate.py`'s header lists
each annotation by pdf-inspector's index.

`src/pdf_comments.rs` pins where each annotation anchors (through the
CropBox and the turn) and checks that each markup anchor covers its words'
ink; the comment-rail flow test and the `pdf-comments-*` headless renders use
it too. To regenerate:

```sh
python -I tests/fixtures/pdf-source/generate.py
```
