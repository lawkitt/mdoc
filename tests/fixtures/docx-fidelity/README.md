# DOCX fidelity check

Eight synthetic legal documents, English and Russian, that score the DOCX
preview against Word's own PDF export at each docxide bump
([originals](../../../docs/specs/originals.md#docx-rendering)). The check is
run by hand; it is not part of the test suite.

```sh
tests/fixtures/docx-fidelity/check.sh                    # score and report
tests/fixtures/docx-fidelity/check.sh --update-baseline  # record new scores
```

It renders each fixture as the Source pane does (the worker's render copy,
bundled fonts, currency-sign fallback), rasterizes both PDFs with gpui-pdf's
hayro at 108 dpi, and prints each fixture's page counts and score: the mean
page SSIM ×100 over the longer document, so a page the other side lacks
scores zero. It fails when a score drops more than two points below
`baseline-<os>.tsv`, the scores recorded at the last bump on that platform
(fonts differ between macOS and Windows, so each keeps its own). A fixture
without a reference PDF is listed for export and not scored.

| Fixture | Covers | Font |
| --- | --- | --- |
| `en-services-agreement` | Multi-level numbering, title, header, page-number footer | Calibri |
| `ru-supply-contract` | Numbered clauses, specification table with a merged cell, ₽ | Times New Roman |
| `en-nda-comments` | Comments over lines, paragraphs, overlapping, a point comment | Cambria |
| `ru-claim-comments` | Russian comments and authors, right-aligned addressee | Calibri |
| `en-memo-hidden-text` | Hidden text in runs, a hidden paragraph, bullets, Letter size | Arial |
| `ru-tariff-tables` | Shaded header rows, vertical merge, totals | Arial |
| `en-letter-footnotes` | Letterhead header, footnotes, a hyperlink, justified text | Times New Roman |
| `ru-power-of-attorney` | Russian lettered sub-items (а), б)), dashes, signature table | Cambria |

## Reference PDFs

The maintainer exports `reference/<fixture>.pdf` from Word for Mac; the
script lists the missing ones. For each fixture:

1. Open it in Word for Mac and choose **Review › No Markup**, so comments
   stay off the page as they do in the Source pane. Leave **Print hidden
   text** off (Word › Preferences › Print).
2. **File › Save As…**, format **PDF**, **Best for printing**, saved as
   `reference/<fixture>.pdf` beside this file.

Then run `check.sh --update-baseline` on that Mac to record the first scores,
and commit the PDFs and `baseline-macos.tsv`. A later bump that lowers a
score by more than two points fails until the drop is explained in its PR.

## Fixtures

`generate.py` writes them with Python's standard library; every sentence was
written for them, and no user document is involved. Regenerate from the
repository root:

```sh
python3 tests/fixtures/docx-fidelity/generate.py
```

SHA-256 of the committed archives:

```text
80bdbdfb8f401a46a71eda3bf2cdb03a9bda111e5502a6ca3f4a6d6bfcae6a50  en-letter-footnotes.docx
624bbdba411cf92ec85150be0c7f952e946a41a878a7b70f08d203fc184f1c9c  en-memo-hidden-text.docx
071ae7349af83e31f23b973cd9e308264431e645040320253fbea3961027ae97  en-nda-comments.docx
4f77c03256348c4455e77cfe3718b805246bfe86b732dbfff62d78c9b9520a6a  en-services-agreement.docx
39dd3b2e25756add2749df4babce0cc677e98dedff3bef0fb1cd2ebc40fa1e3a  ru-claim-comments.docx
b8f3a41790ddcf1997e0adae9012334eb11349dfc42c70997fd57858479be615  ru-power-of-attorney.docx
6ba4796c93a5f062ed6ce17df86db5a9a45baae0b1b9758686c45e5ca094e9b5  ru-supply-contract.docx
fbec4b8ce07cdad0b5f41c8c81053f3e704a9b27342c939960cc6c3109eed993  ru-tariff-tables.docx
```

These are this repository's archives; another Python's zlib may compress the
same parts to other bytes.
