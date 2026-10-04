# Import regression corpus

These fixtures and expected Markdown are copied from MIT-licensed
[AnyDoc](https://github.com/firecrawl/anydoc), revision
`76c444fd1df4df5eb824b55fd3ad144347c839a2` (upstream PR 153).
The license is retained in `LICENSE.anydoc`.

Inputs originate in upstream `tests/fixtures/{doc,docx,xls,xlsx,pdf,malformed}`.
Expected Markdown comes from the corresponding `tests/snapshots` files with
Insta metadata removed. The partly-scanned expected output is the `--skip`
snapshot. Upstream `tests/gen_fixtures.py` and `tests/fixture-src` contain their
generators and sources. They are synthetic test documents, not user files.

The corpus checks converter stability, not visual fidelity. Review output
changes when updating the fork; do not regenerate expectations blindly.

## PDF baseline update, 2026-09-19

PDFs now go directly through the OCR-capable pdf-inspector 1.21.0 fork. The
Office snapshots and AnyDoc dependency retain their previous baseline.
Reviewed changes in `text.pdf.md`: Persian text is in the corrected reading
order, the endnote reference is attached as superscript, and the existing flat
table/list extraction now treats the Table label as inline bold rather than a
heading. That heading/paragraph grouping difference is a known formatting
limitation; table cell text is preserved. This corpus does not promise exact
source layout reconstruction. The partly-scanned snapshot includes additional
blank page separators; readable pages and original warning page numbers remain
unchanged. No source PDFs were modified.

## Selected AnyDoc fidelity fixes, 2026-10-04

Three synthetic fixtures exercise upstream PRs #17, #177 and #174 through mdoc's
actual import boundary. Expected outputs were compared with the old pinned fork
before adoption; every pre-existing expected Markdown file remains unchanged.

- `handmade-nested.docx`: copied byte-for-byte from upstream PR #17, commit
  `35f278e516471cb96841af7e8b0a0b8cdf19649b`. Its generator is the
  `handmade-nested.docx` section of upstream `tests/gen_fixtures.py`. A single-cell
  wrapper now renders its inner grid; the adjacent multi-cell nested table still
  flattens. The expected output retains our base's existing header inference.
- `handmade-checkboxes.docx`: locally generated minimal OOXML package with
  supported Wingdings/Wingdings 2 symbols, private-use and ordinary codes, a styled
  checked run, and an unknown-font symbol after literal parentheses. The source
  XML is retained inside the ZIP. This exercises PR #177 at
  `2f0907d3bb8920e4b44e39443d575db4ce2eb59a` without using application documents.
- `handmade-revisions.doc`: generated using `document` and `run` from upstream
  PR #174's `tests/doc_revisions.rs` at
  `96d59988d6e9f0b946d7849c1d64d80b61680502`. Recipe: deleted `Old clause` with
  `[0x00, 0x08, 1]`, inserted `New clause` with `[1, 8, 1]`, ordinary space,
  `Ordinary strike` with `[0x37, 8, 1]`, and a plain `\r`; no piece modifiers.
  The replacement survives, deleted text disappears, and ordinary strikethrough
  remains. The upstream builder and tests are MIT-licensed; `LICENSE.anydoc`
  covers their attribution here.

The import regression checks exact Markdown, retained source bytes, source-kind
metadata, and warnings. Broader symbol/revision edge cases stay in the fork's
regression tests. See [verification](anydoc-fidelity-verification.md).
