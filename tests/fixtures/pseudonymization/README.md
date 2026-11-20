# Pseudonymization qualification

**No evaluated model passed qualified adoption.** GLiNER2 had the strongest
baseline and passed same-process OCR compatibility, but Russian category and
boundary errors, hidden-source misses and resource growth blocked it. This
is a failed qualification, not a reason to relax the accepted review
contract.

The user then authorized the pinned GLiNER2 checkpoint **experimentally**
despite these blockers. [Inline review verification](inline-review.md)
records that implementation and its safeguards; it neither makes the
qualification a pass nor accepts the proposed targets below.

The [tool instructions](../../../tools/pseudonymization/README.md) reproduce
setup, offline inference, scoring and compatibility. The retained
[measurements](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/evidence/pseudonymization/2026-10-01/measurements.md)
give per-language and per-category counts and each fixture's latency; the
JSON beside them keeps every prediction, miss and setting. They are one
run's measurements, not test snapshots.

## Corpus

Frozen: the headless renders and `mdoc-detect`'s window tests read
`corpus.json`, and its cases are imported, re-marked by the annotation
guide, as [recognition fixtures](../recognition/README.md), which the
detection gate checks. Change those, not this corpus.

`corpus.json` is original synthetic evaluation material, dedicated to
CC0-1.0, with no user documents. Names, organizations, identifiers,
reserved `.invalid` email and URL destinations and addresses are fictional,
chosen to cover the review contract.

OCR fixtures are hand-authored damaged transcripts, not a measured sample
of real OCR errors. Ordinary dates, amounts, legal references and category
words are negative examples. The long fixtures repeat legal clauses; the
stress fixtures hold long compound words that produce many subwords.

`en-settlement-long` is not in the measured comparison. It repeats the
English demo settlement 40 times under section headings (about 80 KB); the
claim number and "Employment Tribunal" are not gold. The app's windows must
give each repeat only its own section's proposals: `mdoc-detect`'s ignored
test `a_long_document_proposes_what_its_sections_do` checks it against the
pinned model in `.qualification/models` (set `MDOC_ONNX_RUNTIME` to a
verified ONNX Runtime library to keep the installed profile out of it).

## Reading scores

`⟦CATEGORY|source text⟧` annotations are removed before inference; the
harness derives half-open UTF-8 byte ranges from the resulting source,
including every exact repeat. An exact boundary **and** category match earns
one true positive. A partial name or address, an omitted organization
affix, included punctuation or a wrong label counts as both a miss and a
false positive. An invalid offset is separate: every prediction must
round-trip to its text at valid UTF-8 boundaries. Duplicates earn nothing.
Raw reports tell complete misses from partial or category errors.

Short-text scores hold 31 EN, 32 RU and 5 mixed-language gold mentions.
Long and stress results are separate, so repeated easy names do not inflate
the baseline. Each category has very few examples: the results can reveal
blockers, not establish general legal-corpus accuracy or anonymity.

## What blocked adoption

- GLiNER2's PII card declares seven languages, not RU, and no organization
  category; a lower threshold did not raise its RU recall.
- `gline-rs` 1.1.0 pins `ort` rc.9 exactly, which cannot share a Cargo graph
  with the OCR's rc.13; adopting it needs a wrapper migration or a
  separately approved worker design.
- Peak memory grew with the subword stress fixtures. Windows x64 and Linux
  remain unmeasured.
- The tool does not qualify the app's review behaviour (tabs, stale
  results, edits, highlights, IME).

## Pins and licences

[models.json](../../../tools/pseudonymization/models.json) pins every
evaluated export, tokenizer and card file to a revision, size and SHA-256;
[provenance.json](../../../tools/pseudonymization/provenance.json) records
the crates' source commits, licence and notice digests and the model-card
pins. Everything evaluated is Apache-2.0 or MIT, except that the
onnx-community multilingual export declares no licence: its inherited terms
need confirming before shipping. A production package keeps the applicable
licences, copyright and NOTICE files (ONNX Runtime's included). These are
practical licence checks, not a training-data audit. No weights are
committed or shipped.

## Next gates

A next comparison keeps this corpus and adds a larger, independently
reviewed legal and OCR holdout with at least 30 examples per category and
language; tuning these fixtures is not holdout accuracy. Proposed targets,
not guarantees: no integrity, Unicode or offline failures; 90% precision
and recall per EN/RU category, 80% on OCR, on that holdout; on an M4,
short-text warm p95 under 250 ms, a 50 KB scan under 10 s, load under 2 s,
peak under 2 GiB. Windows x64 needs the same checks.
