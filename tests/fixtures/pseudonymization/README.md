# Pseudonymization qualification — 2026-10-01

The Rust qualification harness and initial Apple Silicon measurements are
implemented. **No evaluated model passed qualified adoption.**
GLiNER2 has the strongest baseline and passes same-process OCR compatibility,
but Russian category/boundary errors, hidden-source misses and resource growth
prevent adoption on this evidence. This records an unsuccessful qualification
outcome, not a reason to silently relax the accepted review contract.

The user subsequently authorized the pinned GLiNER2 checkpoint **experimentally**
despite these blockers. [Inline review verification](inline-review.md) records
that implementation and its safeguards; this does not change the failed quality
qualification or accept the proposed numerical targets.

The [tool instructions](../../../tools/pseudonymization/README.md) reproduce setup,
offline inference, scoring and compatibility. The
[generated measurements](../../../docs/evidence/pseudonymization/2026-10-01/measurements.md) provide per-language,
per-category counts and each fixture's latency. Adjacent JSON files retain all
predictions, misses, false positives, settings, hashes and process exit evidence.
These are dated measurements, not output snapshots for normal tests.

## Corpus and scoring

`corpus.json` is original synthetic evaluation material, dedicated to CC0-1.0.
It contains no user documents. The pre-existing untracked DOCX/scanned-PDF files
in this checkout were not used. Fictional names, organizations, identifiers,
reserved `.invalid` email/URL destinations and illustrative addresses are
chosen to cover the review contract, not to represent real clients.

Seventeen fixtures cover EN/RU legal text, repeated/inflected names, initials,
email/telephone/address/identity/tax/bank details, tables, link destinations,
image paths, inline/fenced code, HTML, Cyrillic/Ё, emoji and decomposed accents.
OCR fixtures are hand-authored damaged transcripts (digit/letter substitutions,
split words and mixed alphabets), not a measured sample of real OCR errors.
Ordinary dates, amounts, article/section/case references and category words are
negative examples. Two long fixtures repeat legal clauses 64 times; two stress
fixtures contain long compound words that produce many subwords.

`⟦CATEGORY|source text⟧` annotations are removed before inference. The harness
derives half-open UTF-8 byte ranges from the resulting immutable source, including
all exact repeats. An exact boundary **and** category match earns one true
positive. Partial names/addresses, omitted organization affixes, included
punctuation and wrong labels count as both a miss and a false positive. These
are distinct from an invalid offset: every prediction must round-trip to the
reported text at valid UTF-8 boundaries. Duplicate predictions earn no extra
credit. Raw reports distinguish complete misses from partial/category errors.

Short-text scores contain 31 EN, 32 RU and 5 mixed-language gold mentions.
Long/stress results are separate so repeated easy names do not inflate the
baseline. Each category has very few examples: these results can reveal
blockers, but cannot establish general legal-corpus accuracy or anonymity.

## Measured comparison

Apple M4, 24 GiB RAM, macOS 15.7.7, Rust 1.98.1; release builds, CPU only,
four inference threads, network denied with the macOS sandbox. Models were
downloaded and verified beforehand. Candidate runs were sequential, with no
concurrent builds. Load time excludes artifact hashing; warm latency is the
median of the second/third calls across the thirteen short fixtures.

| Engine / export | Threshold | EN precision / recall | RU precision / recall | Setup MB | Load ms | Warm median ms | Peak RSS MiB |
| --- | ---: | --- | --- | ---: | ---: | ---: | ---: |
| gline-rs / multi v2.1 Q8 | 0.5 | 72.7% / 51.6% | 78.6% / 34.4% | 365.45 | 718 | 31.7 | 1913 |
| gline-rs / multi v2.1 Q8 | 0.3 | 48.6% / 54.8% | 58.1% / 56.2% | 365.45 | 672 | 32.5 | 1898 |
| gline-rs / Knowledgator PII Q8 | 0.5 | 95.2% / 64.5% | 66.7% / 37.5% | 205.43 | 332 | 22.5 | 2826 |
| gline-rs / Knowledgator PII Q8 | 0.3 | 80.6% / 80.6% | 48.5% / 50.0% | 205.43 | 329 | 25.8 | 3452 |
| gliner2-rs / privacy PII FP16 | 0.5 | 81.2% / 83.9% | 73.3% / 68.8% | 631.52 | 947 | 132.4 | 2266 |
| gliner2-rs / privacy PII FP16 | 0.3 | 77.1% / 87.1% | 68.8% / 68.8% | 631.52 | 860 | 139.8 | 2315 |

Setup sizes include selected export files, tokenizer, configuration/card files;
they exclude runtime/tool binaries. Peak RSS includes the pathological subword
stress fixtures and all previous calls. It does not show idle retention or
prove an arbitrary long document fits a fixed memory budget.

The 24,512-byte English / 48,960-byte Russian long fixtures took approximately
2.8–3.8 s for multilingual Q8, 1.8–3.4 s for Knowledgator Q8, and 7.7–8.9 s
for GLiNER2. Original document offsets remained valid across windows, including
the tail; valid offsets do not establish detection coverage. All six processes
exited cleanly, with no invalid byte ranges or repeated-call nondeterminism.

At threshold 0.5, GLiNER2 missed 5/31 EN and 10/32 RU gold mentions, with
6 EN and 8 RU false positives. Examples include underscore-separated names
inside Markdown destinations/image paths; Russian INN misclassified as identity
or telephone; missed SNILS/address values; stripped ООО/quotes; a trailing full
stop included in an organization; case numbers classified as bank accounts;
and bare words such as `person`, `organization` and `адрес` highlighted as PII.
Lowering its threshold did not improve RU recall. Its PII card declares seven
languages excluding RU and does not include an organization category; both were
explicitly evaluated rather than assumed to work from the multilingual name.
[Pinned export card](https://huggingface.co/jugaadsrl/gliner2-privacy-filter-PII-multi-onnx/blob/e594898629d452e8311796f5f329c7edbeda907c/README.md).

## Runtime, setup and lifecycle evidence

All candidates loaded and inferred offline using the **installed OCR ONNX
Runtime 1.27.0** library, with its expected SHA-256. GLiNER2 additionally passed
[same-process coexistence](../../../docs/evidence/pseudonymization/2026-10-01/ocr-coexistence.json): the pinned
pdf-inspector OCR engine produced matching EN/RU scanned-PDF transcripts before
PII load, while both were loaded, and after PII drop, with unchanged PDF bytes.

`gline-rs` 1.1.0's exact `ort` rc.9 dependency cannot resolve in the same Cargo
graph as pdf-inspector's exact rc.13; the
[resolution failure](../../../docs/evidence/pseudonymization/2026-10-01/gline-ocr-resolution.log) is retained.
Standalone inference with the newer native library does not remove this blocker.
Adopting it would require an upstream/fork wrapper migration or a separately
approved worker design. GLiNER2 0.9.6 resolves with OCR's rc.13; its Hub feature
is disabled for qualification.

The Rust setup command was exercised against a fresh Knowledgator cache and
then repeated under a network block. It verified and reused all files. Unit
tests reject missing/partial/same-size-corrupted artifacts, malformed corpus
annotations, invalid Unicode offsets and invalid window geometry. Process tests
verify killing/reaping an uncooperative child and refusing to reuse a prior
report. A three-second actual GLiNER2 scan was terminated and reaped with no
inference report accepted; the cancellation process/log evidence is retained.

The tool never writes document text or replacements. App tab switching,
revision/stale-result checks, review edits/undo, mappings, native highlights,
keyboard/IME and popup layout belong to the subsequent inline milestone; this
tooling does not qualify those unimplemented behaviors. Windows x64 and Linux
inference/native operation remain unmeasured. No production dependency or
platform support policy has changed.

## Exact pins and licenses

[models.json](../../../tools/pseudonymization/models.json) pins every evaluated
export/tokenizer/card file to a revision, size and SHA-256.
[provenance.json](../../../tools/pseudonymization/provenance.json) records exact
crate source commits, license/notice file digests and upstream model-card pins;
three tool Cargo lockfiles retain registry checksums and transitive versions.

| Component | Evaluated version/revision | Declared license / finding |
| --- | --- | --- |
| gline-rs / orp | 1.1.0 / 1.0.0; source commits in provenance | Apache-2.0; license files checked |
| gliner2-rs | 0.9.6 / `7626d237d8c58f0fcfadd094e8360e7d5ae9c8ee` | Apache-2.0; LICENSE and NOTICE checked |
| ort / ort-sys | rc.9 and rc.13 | MIT OR Apache-2.0; both license files checked |
| tokenizers code | 0.21.4 / 0.23.2 | Apache-2.0; license files checked |
| GLiNER multi base model | `443d26d654e0324125a96bebd8e796c14ff2efe6` | Apache-2.0 in upstream model card |
| Multilingual ONNX export / tokenizer | onnx-community `6ddaeb9413b0e71ad8457da1aab378a165b24058` | Export card identifies the base, but has no explicit license declaration; inherited terms/provenance need confirmation before shipping |
| Knowledgator PII export / tokenizer | `61726e0ad791dcab3e29339bbec3ad42ded65641` | Apache-2.0 in model card; English declared; retained fine-tuned tokenizer includes special tokens |
| GLiNER2 privacy base | Fastino `1cb4166094dc58fa8d836429f060d6c95f62b495` | Apache-2.0 in model card |
| GLiNER2 ONNX export / tokenizer | jugaadsrl `e594898629d452e8311796f5f329c7edbeda907c` | Apache-2.0 in export card; derivative weights keep upstream obligations |
| Base encoder/tokenizer lineage | Microsoft mDeBERTa-v3-base / DeBERTa-v3-small; exact card pins in provenance | MIT in Microsoft cards; fine-tuned tokenizer bytes are separately pinned |
| Native ONNX Runtime | 1.27.0, existing app pin/digest in `src/ocr.rs` | MIT; existing runtime license/third-party notices must travel with redistribution |

The original multilingual model provides no ONNX artifact; evaluation uses the
identified third-party quantized export. These are practical source/license
checks, not a claim that all training data or derivative provenance has been
audited. A production package must retain applicable licenses, copyright and
NOTICE files and revisit the export attribution gap. No weights are committed
or added to mdoc's shipped distribution.

## Adoption decision and next gates

The initial qualification recommended keeping inline pseudonymization pending.
The later experimental exception is recorded above. GLiNER2 is the best runtime integration
candidate, but this particular PII checkpoint is not qualified for the accepted
EN/RU and whole-source scope. A next model/export comparison should preserve
this corpus and add a larger independently reviewed legal/OCR holdout with at
least 30 examples per required category/language. Label, boundary and subword
policy changes need fresh evidence; tuning these fixtures is not holdout accuracy.

Based on this baseline, propose the following adoption targets for review:
zero integrity/Unicode/source/offline failures; at least 90% precision and recall
per EN/RU native category and at least 80% on OCR categories on that holdout;
on the reference M4, short-text warm p95 below 250 ms, 50 KB legal-source scan
below 10 s, load below 2 s, and process peak below 2 GiB including stress input.
These are proposed evaluation targets, not accepted product guarantees. Resource
limits must use actual tokenizer lengths and remain cancellable. Windows x64
needs equivalent offline/runtime and language checks before adoption; native
review UI still needs its own acceptance after implementation.
