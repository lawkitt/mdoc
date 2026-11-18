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
cf14cb244c1742fdfd8a98ee8d0055f1e427322b4a725cdd5fee10ebdaf0dbff  en-letter-footnotes.docx
6eafc047443c01d68541eed2ca6f13462e57f8dbafd3d6b5049c93b341d8b7cb  en-memo-hidden-text.docx
3b1d55a4532c32dab6d371089bbcb39562700c826ce8bfa797fab333ee93701a  en-nda-comments.docx
dc7172f5dfc9be78b6702d0df8d2bcbc5e0ccd0e089a1f900ba75abc3065826b  en-services-agreement.docx
ca5d11bf440565461df000802562ed8a12254782047d727c68e817721cea1343  ru-claim-comments.docx
7cd09209489b622dd10e5fb04ea3c6bbf60817b7b1b2dec34c175ca9eba500f9  ru-power-of-attorney.docx
ef179aaae0c8f38d6121613e65b4f17613e4004089d162234c9d373c48211557  ru-supply-contract.docx
5393991d977ee0bc6c7e3fc7c4dbf22e9f056916b3d82a47a629206e19d41ad8  ru-tariff-tables.docx
```

These are this repository's archives; another Python's zlib may compress the
same parts to other bytes.
