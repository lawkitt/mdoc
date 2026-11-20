# Import regression corpus

Synthetic test documents, not user files; `<name>.md` beside one is its
expected Markdown. `crates/mdoc-import/src/import.rs` tests them through
mdoc's import; `crates/mdoc-import/src/resume.rs` runs the partly-scanned
and running-header PDFs through the phones' page-at-a-time import.

The corpus checks converter stability, not visual fidelity, and does not
promise exact source layout. Review every output change when updating a
fork; do not regenerate expectations blindly.

## From AnyDoc

Fixtures with no generator and no entry below are copied from MIT-licensed
[AnyDoc](https://github.com/firecrawl/anydoc) at
`76c444fd1df4df5eb824b55fd3ad144347c839a2`, from its
`tests/fixtures/{doc,docx,xls,xlsx,pdf,malformed}`. `LICENSE.anydoc` keeps
the licence and covers every AnyDoc-derived file and builder here. Expected
Markdown is upstream's `tests/snapshots` with Insta metadata removed; the
partly-scanned one is the `--skip` snapshot. Upstream's
`tests/gen_fixtures.py` and `tests/fixture-src` hold their generators and
sources.

PDFs go through the pdf-inspector fork, not AnyDoc, so PDF expectations
follow that fork. Known limit in `text.pdf.md`: the Table label comes out
as inline bold, not a heading; the table's cell text is kept.

- `handmade-nested.docx`: byte-for-byte from upstream PR 17 at
  `35f278e516471cb96841af7e8b0a0b8cdf19649b`; its generator is the
  `handmade-nested.docx` section of upstream `tests/gen_fixtures.py`. A
  single-cell wrapper renders its inner grid; the adjacent multi-cell
  nested table still flattens. The expected output keeps our base's header
  inference.
- `handmade-checkboxes.docx`: a minimal OOXML package made locally, no
  application involved; its source XML is inside the ZIP.
- `handmade-revisions.doc`: built with `document` and `run` from upstream
  PR 174's `tests/doc_revisions.rs` at
  `96d59988d6e9f0b946d7849c1d64d80b61680502`. Recipe: deleted `Old clause`
  with `[0x00, 0x08, 1]`, inserted `New clause` with `[1, 8, 1]`, an
  ordinary space, `Ordinary strike` with `[0x37, 8, 1]`, and a plain `\r`;
  no piece modifiers.
- `handmade-hidden.{docx,rtf,doc}`: byte-for-byte from the lawkitt/anydoc
  fork at `520ae174d75e85bc8e1edc03678cdefdf5f2a126`
  (`tests/fixtures/{docx,rtf,doc}`), made by `hidden_docx`, `hidden_rtf`
  and `hidden_doc` in its `tests/gen_fixtures.py`; expected Markdown is its
  snapshots without Insta metadata. Every hidden word is `SECRET-*`.
- `handmade-comments.*`: byte-for-byte from AnyDoc `5e22f0a2`'s fixtures;
  `text_annotations.pdf` from pdf-inspector `957693f5`'s (both MIT).

Broader symbol and revision edge cases stay in the AnyDoc fork's tests.

## Generated here

Each `gen_*.py` writes its fixture beside itself; its header says what the
pages hold. Run with Python 3 (standard library only):
`python3 tests/fixtures/import/gen_running_header.py`.

| Generator | Fixtures |
| --- | --- |
| `gen_hidden_content.py` | `handmade-hidden-content.docx` |
| `gen_running_header.py` | `handmade-running-header.pdf` |
| `gen_scanned_running_header.py` | `handmade-scanned-running-header.pdf` |
| `gen_sparse_page_header.py` | `handmade-sparse-page-header.pdf`, `handmade-scanned-sparse-page-header.pdf` |
| `gen_mixed_size_header.py` | `handmade-mixed-size-header.pdf`, `handmade-scanned-mixed-size-header.pdf` |
| `gen_sparse_page_closest_header.py` | `handmade-sparse-page-closest-header.pdf` |

Native-text output is deterministic. The scans are rasterized by macOS
`sips` with seeded page offsets, so only a different `sips` changes their
bytes, and they can only be regenerated on a Mac. The later generators
import `gen_scanned_running_header.py` (and the closest-header one
`gen_sparse_page_header.py`); rerunning the earlier generators with the
current shared code gives byte-identical fixtures.

`handmade-hidden-content.docx.md` holds no name because the pinned AnyDoc
reads no comment or property parts and skips `w:del` and `w:moveFrom`,
with no option to keep them.
