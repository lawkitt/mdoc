# Hybrid PII evaluation — 2026-10-07

Frozen isolated evaluation. Q15 subsequently selected email and contextual
checksum-valid INN/SNILS for experimental production use.

## Exact scoring

Each cell is **true positives / false positives / misses**. A hit requires
an exact half-open UTF-8 source range and category; partial/category errors
count as a false positive and a miss. Model outputs use the same research
overlap resolver as the hybrid. This measures detection, before the app's
source-syntax protection, exact-repeat expansion and replacement policy.

| Cohort | Fixtures | Gold | GLiNER2 | Rules | Full hybrid | Selected hybrid |
| --- | ---: | ---: | --- | --- | --- | --- |
| Existing short corpus | 13 | 68 | 52 / 15 / 16 | 16 / 0 / 52 | 52 / 15 / 16 | 52 / 15 / 16 |
| Calibration | 8 | 14 | 12 / 6 / 2 | 13 / 0 / 1 | 14 / 4 / 0 | 14 / 4 / 0 |
| Holdout, including four negative fixtures | 12 | 24 | 15 / 19 / 9 | 17 / 0 / 7 | 21 / 16 / 3 | 21 / 16 / 3 |
| All | 33 | 106 | 79 / 40 / 27 | 46 / 0 / 60 | 87 / 35 / 19 | 87 / 35 / 19 |

Selected hybrid includes email, contextual checksum-valid INN and SNILS.
It achieves the full hybrid's exact counts on every fixture, with no
changes to the existing 13 fixtures. This is a small researcher-authored
synthetic sample, not independent real-document qualification.

## Individual rule ablations

Each row adds only that recognizer to the captured GLiNER2 predictions.
The model baseline is 79 / 40 / 27 across 106 expected mentions.

| Rule | TP / FP / misses |
| --- | --- |
| rule:email-format | 82 / 40 / 24 |
| rule:iban-mod97-country-length | 79 / 40 / 27 |
| rule:inn-checksum-context | 80 / 39 / 26 |
| rule:phone-format-context | 79 / 40 / 27 |
| rule:ru-account-format-context | 79 / 40 / 27 |
| rule:ru-bic-format-context | 79 / 40 / 27 |
| rule:snils-checksum-context | 83 / 36 / 23 |

## Category and language detail

| Language | Gold | GLiNER2 | Selected hybrid |
| --- | ---: | --- | --- |
| en | 36 | 31 / 8 / 5 | 31 / 8 / 5 |
| mixed | 23 | 18 / 12 / 5 | 22 / 11 / 1 |
| ru | 47 | 30 / 20 / 17 | 34 / 16 / 13 |

| Category | Gold | GLiNER2 | Selected hybrid |
| --- | ---: | --- | --- |
| ADDRESS | 4 | 2 / 2 / 2 | 2 / 2 / 2 |
| BANK | 14 | 14 / 10 / 0 | 14 / 9 / 0 |
| EMAIL | 16 | 13 / 1 / 3 | 16 / 1 / 0 |
| IDENTITY | 11 | 5 / 3 / 6 | 9 / 3 / 2 |
| ORG | 8 | 4 / 10 / 4 | 4 / 10 / 4 |
| PERSON | 27 | 21 / 1 / 6 | 21 / 1 / 6 |
| PHONE | 13 | 13 / 10 / 0 | 13 / 6 / 0 |
| TAX | 13 | 7 / 3 / 6 | 8 / 3 / 5 |

## Timing and execution

Apple M4, 24 GiB RAM, macOS 15.7.7, Rust 1.98.1; release CPU, four model threads.
- Cached regex construction: 6.103 ms.
- Rules scan, median of 33 fixture medians (101 samples each): 0.001542 ms.
- Hybrid merge, median of one measurement per fixture: 0.000708 ms; too small for a firm budget.
- Rules stress: 2,097,152 bytes, 20,000 matches, median 11.587 ms over seven calls; includes overlap resolution and offset validation, excludes editor/model work.
- GLiNER2 artifact verification: 309.6 ms; engine load: 951.9 ms; whole-process peak RSS: 1746.6 MiB.
- Each short fixture ran three model calls in one loaded engine; all source offsets valid and outputs deterministic. Process exited successfully without deadline expiry under a macOS deny-network sandbox.
- Adapter uses the current app's label families, threshold 0.5, FP16 and 512 actual schema/text-token bounded windows (128 words, 32 overlap). Engine reuse here differs from the app's scan lifecycle; these are not end-to-end app timings.

## Limits and decision proposal

Q15 confirmed adding only email + contextual checksum-valid INN/SNILS experimentally.
Phone, IBAN and RU bank-format rules produced no additional exact hits.
The selected hybrid still has 35 false positives and 19 misses overall.
Rules had zero false positives on these fixtures, which does not establish general precision.

Context only looks backward on the same line, up to 80 characters: table
headers and labels such as 'ИНН физлица' are not recognized. Invalid/OCR
identifiers remain sensitive gold; checksum rejection never suppresses a
non-overlapping model detection. Historical SNILS formats, registry
existence, broad email syntax and international phone formats remain
unqualified. Larger representative EN/RU corpora and native editor/history
performance need separate verification. Existing long/stress model fixtures
were excluded from this detector comparison; only the rules stress ran.

Holdout text and gold were frozen before first inference. No recognizer
was retuned after inspecting holdout predictions. The stress padding was
corrected from an oversized first run, then rerun at exactly 2 MiB; this
did not change any corpus fixture or detection rule.

## Raw evidence

[Comparison](../../../../tests/fixtures/pseudonymization/hybrid/comparison.json), [model](model.json), [process](model.process.json),
[stderr](model.stderr.log), [source and binary hashes](provenance.json).
Every miss and false positive, including the four holdout negatives, is
retained per fixture in the comparison JSON. See the [corpus and commands](../../../../tests/fixtures/pseudonymization/hybrid/README.md).
