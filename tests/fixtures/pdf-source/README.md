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

## Encrypted twins

`comments-aes128.pdf` (security handler V4, R4, AES-128) and
`comments-aes256.pdf` (V5, R6, AES-256) are `comments.pdf` encrypted by
lopdf with the user password `Petrov-2026` and the owner password
`Petrov-2026-owner`, published test values that protect nothing real.
Neither opens without a password; PDFKit opens both with either one.
`comments-owner-only.pdf` (V4, R4, AES-128) has no user password and
allows only printing and copying: a lock against editing, which opens with
no password.
gpui-pdf's `encrypted_tests` check that each renders like `comments.pdf`,
light and dark, and the `pdf-encrypted` headless renders show the AES-256
one in the Original. To regenerate them after `comments.pdf` changes:

```sh
cargo test -p gpui-pdf --lib -- --ignored write_encrypted_fixtures
```

Random salts make every run's bytes differ.

## A locked scan

`partly-scanned-aes256.pdf` is `tests/fixtures/import/handmade-partly-scanned.pdf`
encrypted the same way (V5, R6, AES-256, user password `Petrov-2026`, owner
password `owner-2026`) by mdoc-import's test helper `encrypted`. A locked PDF
with pages that need OCR: the desktop's import stops for OCR consent with
its password kept, and the Original then opens with it.
