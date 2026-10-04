# pdf-inspector upstream synchronization

Status: decisions Q1-Q4 and final shared understanding accepted, 2026-10-04.
The user confirmed the public corpus and authorized implementation with “agree”.

## Verified starting point

- mdoc pins lawkitt/pdf-inspector at
  `7a11f9f3f2423edd0035f87e7152b49c08a7ce1a` with OCR enabled.
- The local fork has two commits beyond upstream's shared 1.21.0 baseline:
  pinned Cyrillic models/explicit runtime paths, and optional raw recognition.
- Fetched upstream main is `ef52f77850b29189048797a48b24862251f54fe7`
  (1.25.2), with 33 intervening commits. Freeze this target for the update.
- A merge-tree simulation reports one textual conflict, in `src/bin/pdf2md.rs`.
  A clean textual merge elsewhere does not establish API or output correctness.
- Upstream changes font decoding, bidirectional reading order, scan detection,
  text metadata, embedded CMaps, malformed-document repair and OCR rendering.

## Decision tree

Round 1, confirmed by the user with “all recommendations”:

1. Q1: merge the complete frozen upstream revision, retaining fork history.
   Preserve Cyrillic manifests, explicit runtime paths and raw recognition.
2. Q2: update both fork and mdoc through review branches/PRs. mdoc uses an exact
   tested fork commit. Direct adoption on main is outside this delivery scope.
3. Q3: require fork formatting/lint/tests with OCR enabled, mdoc checks, EN/RU
   OCR and representative legal-PDF before/after comparisons. Unexplained text
   loss, reading-order errors and consent/offline/raw-output regressions block
   adoption. Review structural improvements individually. macOS and Windows
   OCR smoke checks are required before adoption, with unavailable checks
   explicitly pending. Automated checks do not establish native acceptance.
   - User refinement: source good OCR examples from the internet/open databases.
   - Q4 confirmed: the 12-page public corpus, checked references, pinned sources
     and per-page comparisons; macOS and Windows remain adoption gates.

## Public fixture contract — Q4, accepted

Use a small, pinned 12-page external corpus alongside existing synthetic EN/RU
qualification fixtures:

- Six Russian printed business/legal pages from
  [MWS Vision Bench](https://huggingface.co/datasets/MTSAIR/MWS-Vision-Bench).
  Its public metadata contains 144 full-page OCR records, including 86 business
  records. Concrete candidates include `image_0.jpg` (official resolution,
  record 3), `image_2.jpg` (customer obligations, record 12), `image_3.jpg`
  (assignment agreement, record 16), and `image_6.jpg` (contract, record 28).
  Select two more for small text/table or imperfect-scan coverage after visual
  inspection. Exclude handwriting from the required corpus.
- Four English law/regulation, tender or financial pages from
  [DocLayNet](https://github.com/DS4SD/DocLayNet). It supplies matching page
  PDFs, images, layout annotations and digital text cells. Keep original PDFs
  for native extraction; derive image-only PDFs for controlled OCR checks.
  These rasterized pages are not evidence of genuine scan degradation.
- Two genuine English legal scan pages from the Internet Archive's
  [1917 Supreme Court digest](https://archive.org/details/digestdecisions01compgoog),
  a concrete source also indexed by
  [DocuBench](https://github.com/DocuPipe/DocuBench/blob/main/SOURCES.md).
  Verify source rights and actual page suitability before admitting excerpts.
  Archive OCR is a draft reference, not authoritative ground truth.

The MWS dataset card declares CC BY 4.0; its repository's MIT license covers
software and documentation and is not a substitute for dataset attribution.
DocLayNet supplies CDLA-Permissive-1.0. Record exact source revisions/URLs,
source page or item IDs, SHA-256, licenses/notices and any PDF derivation.
Admit only items with a clear reuse basis; choose a comparable replacement if
an item's rights, access or transcript cannot be established.

Visually check references against the actual page before scoring. Preserve
original annotations alongside corrections. Do not treat extracted native text,
archive OCR, or candidate model output as unquestioned ground truth. Deduplicate
MWS task records by image. Compare the same input bytes, model bundle, DPI and
confidence settings at both fork revisions, keeping raw recognition separate
from final Markdown. The existing synthetic Ё/ё coverage remains mandatory.

Report character and word error rates per page, missing/duplicated blocks,
reading order and critical numbers/clause identifiers. Investigate each
regression rather than hiding it in corpus averages or resetting snapshots.
This is a focused update gate, not general OCR or table qualification.

## Delivery sequence

1. Pin/admit fixtures and retain baseline outputs before merging.
2. Merge the frozen target on a review branch; reconcile the CLI conflict and
   preserve fork semantics; run fork-required checks, including available
   upstream evaluation requirements, reporting any unavailable external suite.
3. Compare corpus outputs and resolve unexplained regressions.
4. Prepare the fork PR and update mdoc's exact dependency revision/lockfile on
   its review branch. Run integration and platform smoke checks.
5. Deliver both PRs with evidence and any adoption blockers clearly stated.

## Implementation evidence

The full merge preserves the fork's local extensions. The CLI conflict retains
upstream's positional parser and registers the fork's `--ocr-model` value option
with a regression test. mdoc pins tested fork
`1baba87892a929e64d08069cb223b4a312f755cf`.

The [final public corpus](../../tests/fixtures/ocr-upstream/README.md) contains
six Russian printed contract/business photos, two English law pages, two English
financial pages and two English historical legal scans. Initial Russian
candidates with handwriting or incomplete references were replaced during visual
inspection. Four DocLayNet native originals and existing synthetic EN/RU PDFs
provide 18 captures in total. All 54 prepared/raw/provenance artifacts are
byte-identical to baseline; retained per-page CER/WER report baseline limitations.

See [verification](../../tests/fixtures/ocr-upstream/verification.md) for actual
checks and pending Windows/remote-CI/external-suite acceptance. The corpus is a
regression gate, not general OCR qualification.

## Glossary

- Reference transcript: text checked against the source image, with a stated
  provenance and correction history; independent of the candidate OCR output.
- CER/WER: character/word edit distance divided by reference length, with the
  normalization policy recorded and identical across compared revisions.
- Controlled rasterization: a digital source page rendered into an image-only
  PDF for OCR testing; it does not reproduce a physical scanner's defects.
