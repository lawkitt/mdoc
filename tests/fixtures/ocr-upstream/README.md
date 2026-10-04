# Public corpus for the pdf-inspector 1.25.2 update

A focused regression corpus: 12 public pages, four matching native originals,
and the existing two CC0 synthetic EN/RU fixtures. Recognition remains in the
Rust fork and runs offline. This does not qualify general OCR/table accuracy.

## Sources and rights

`manifest.json` records pinned source URLs/revisions, item/page IDs, source and
fixture/reference SHA-256, derivations, runtime digests and tool versions.

- Six Russian legal/business photos: education contract, apartment handover,
  franchise termination, employment duties, goods quality and confidentiality.
  Source: MWS Vision Bench, MTS AI Research, revision
  `8f9e77a4b329b930698e94874b98f482ff00192d`; record IDs are in the manifest.
  [Dataset card](https://huggingface.co/datasets/MTSAIR/MWS-Vision-Bench).
  Data is offered under CC BY 4.0; retain attribution and the included license.
  The separate benchmark software's MIT license is not the dataset license.
- Four English DocLayNet pages: Botswana constitution pages 4/34, NASDAQ FFIN
  2002 report page 9 and NASDAQ EEFT 2000 report page 12. Source: IBM Research's
  [DS4SD/DocLayNet](https://github.com/DS4SD/DocLayNet), release 1.0.0.
  CDLA-Permissive-1.0 and original copyright notices are retained. Archive entry
  hashes identify the pages. `*.native.pdf` are originals; corresponding OCR
  PDFs are controlled rasterizations, not genuine physical scans.
- Two image-only scan pages from the 1917 *Digest of decisions of the Supreme
  Court of the United States*, printed pages 488/497. Original
  [Internet Archive item](https://archive.org/details/digestdecisions01compgoog).
  The pinned DocuPipe/DocuBench excerpt `eeqkpCkH` supplies pages 1/10, copied
  without image re-encoding. A 1917 U.S. publication is treated as public domain;
  [DocuBench records this basis](https://github.com/DocuPipe/DocuBench/blob/43a3f3bc00e591e711075678ca6d154acfedcf42/SOURCES.md).

MWS JPEGs were embedded without cropping/pixel edits/text layers. Publisher
redactions remain. Handwriting is outside required coverage. Candidate image 0
was excluded for handwritten date/number fields; image 2 was excluded because
its nominal full-page reference covered only the photographed page's bottom.
Candidates named in the design record are historical, not the final corpus.

## References and limits

`annotations/` retains source records/cells and the independent Archive OCR draft
for the scans. Adjacent `.txt` references were inspected and adjusted by the agent
against each selected image; they are **not independently human-verified gold
transcripts**. Blank redaction placeholders and non-image heading underlines
were removed from MWS annotations. The goods-quality reference restores the
omitted continuation of clause 7.6 and corrects the time-range preposition.
DocLayNet cells were reordered for caption/body/sidebar/footer reading order,
and the drop cap was joined. Scan draft corrections are in `*.review.json`.

Per-page CER/WER remain sensitive to annotation errors, punctuation, superscripts
and reading order. Dense historical scans and the financial sidebar exhibit
substantial pre-existing reading-order errors at both revisions. Unchanged
output is not evidence of good recognition. Prepared Markdown and raw OCR remain
separate so fusion cannot conceal recognition changes; confidence is not accuracy.

## Reproduce

The fork's `examples/ocr_corpus.rs` runs offline Force OCR on image-only fixtures
and Off extraction on `*.native.pdf`, validates one-page routing/raw retention,
and confirms unchanged source bytes. Use the same harness, models, confidence 0
and 150 DPI at both revisions. `RUNTIME_ROOT/models/` must point to the explicit
Cyrillic artifact directory, not the parent model-cache root. PDFium and ONNX
libraries reside directly in `RUNTIME_ROOT`; no runtimes/models are checked in.

```sh
cargo run --manifest-path /path/to/pdf-inspector/Cargo.toml --features ocr \
  --example ocr_corpus -- tests/fixtures/ocr-upstream OUTPUT RUNTIME_ROOT
cargo run --manifest-path /path/to/pdf-inspector/Cargo.toml --features ocr \
  --example ocr_corpus -- tests/fixtures/ocr-qualification OUTPUT RUNTIME_ROOT
python3 tools/ocr-upstream/compare.py BASELINE UPDATED --output comparison.json
```

The standard-library scorer verifies input/reference digests and uses exact
Levenshtein distance. Normalization applies NFKC, removes soft hyphens/Markdown
heading or emphasis wrappers, joins line-end word breaks and collapses whitespace;
case/substantive punctuation remain. Any changed artifact requires review, and
synthetic references must match. Distance was independently cross-checked against
dynamic programming on 961 string pairs.

## Recorded evidence — 2026-10-04

`captures/baseline/` and `captures/updated/` retain all 54 artifacts from 18
captures; `comparison.json` has per-page scores and artifact hashes. Baseline:
`7a11f9f3f2423edd0035f87e7152b49c08a7ce1a`. Merged library state:
`ae4d4e82c4156e81a093c70930201b4dd3178a33`; subsequent example/evidence-only
commit `1baba87892a929e64d08069cb223b4a312f755cf` is mdoc's dependency pin.
All 54 artifacts are byte-identical. Synthetic EN/RU raw/prepared text matches
after normalization, including Ё/ё. Runtime digests match the app's pinned files.

App/platform checks are in `verification.md`. The separate upstream `pdf-evals`
suite is absent/inaccessible, so full snapshot/semantic scoring is unverified.
These captures do not establish native UI acceptance.
