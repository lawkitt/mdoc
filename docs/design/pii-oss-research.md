# Rust PII library research — 2026-10-07

Status: source inspection and isolated evaluation complete; Q15 confirmed the
email/contextual INN/SNILS subset. Architectural refinements are implemented. The
source-review recommendations below do not imply
authorization to add production detectors or replace the agreed domain model.

## Conclusion

Borrow narrow engineering techniques and evaluate structured recognizers.
Keep mdoc's occurrence provenance, revision checks, source protection, Keep,
grouping and undo lifecycle app-owned. None of the inspected core implementations
provides the agreed live-editor restoration/history contract.

The most useful immediate refinement is a validated single-pass replacement
builder that computes both input and resulting output ranges. A potential later
detector improvement is a small explicit combination of structured recognizers
and current GLiNER2, evaluated first on EN/RU legal/Markdown positives and negatives.

## Method and evidence limits

Read current docs and cloned default-branch source into a temporary research
directory. Inspected manifests/license files, core detection and anonymization
code, tests, overlap resolution, offsets and runtime dependencies. Pins below
identify the inspected code rather than assuming latest docs and repository
HEAD are always equivalent.

No production dependency, model, detector or application behavior was changed.
No full upstream test suite, model inference or comparative speed/accuracy
benchmark was run. One isolated source-extracted helper probe was compiled and
run to verify the Unicode defect described below. Marketing speed ratios and
recognizer counts are not mdoc qualification evidence.

## Inspected projects

| Project | Inspected version / revision | Declared license | Useful material | Adoption assessment |
| --- | --- | --- | --- | --- |
| censgate/redact, redact-core | 0.12.5 / `6d10a140a8400188704399b1385fb2df89035736` | Apache-2.0 | Recognizer/strategy separation, recognizer identity, pattern/context/validation stages, append-based output construction | Borrow concepts; whole core adds crypto/policy/data surfaces we do not need. NER runtime binding differs from mdoc. |
| qooba/anonymize-rs | 0.0.2 / `a73bff5bfee2df29db19816b62888b000314d80b` | Apache-2.0 | Separate NER, regex and keyword stages; originals/placeholder association | Architectural reference only. Current head dates to 2023-08-05; unconditional web/async/tract dependencies and a verified UTF-8 helper defect make direct adoption unattractive. |
| kadir/cloakrs | 0.4.1 / `765676a6355fecc746105b65376d5aa25fcb7033` | MIT | Explicit half-open byte spans, recognizer IDs, checksum-backed recognizers, context and invalid-span tests | Strong source for selected validators/tests. Adapt boundary and hidden-source policy; do not import the entire locale/entity catalogue. |
| jqueguiner/presidio-rs | 0.1.11 / `d3b83a1746a404134a10ad6c2d3d2eda35f523d4` | MIT | Separate analysis/operators, result output spans, context and checksum validators, Russian SNILS recognizer | Strong reference for batch/output range bookkeeping and selected recognizers. RU registration and optional runtime require scrutiny. |
| Jiansen/pii-vault | 0.3.0 / `e5fc84c3848c017668ec0149080a10b421b37d51` | MIT | Value index, per-vault scope, explicit original/replacement result records | Keep scoping/index concepts. Global token detokenization and serializable vault do not meet the occurrence/lifetime contract. |

Verified license declarations and files at these revisions. Any actual source
copy should carry its upstream attribution and applicable notices; ideas and
small app-specific abstractions can instead be implemented around our contracts.

## Detailed findings

### redact-core

[Recognizer registry](https://github.com/censgate/redact/blob/6d10a140a8400188704399b1385fb2df89035736/crates/redact-core/src/recognizers/registry.rs)
uses shared recognizer instances and an entity-to-recognizer index. Results
carry recognizer names, scores, source spans and optional context. This suggests
compact app-owned recognizer IDs and typed evidence rather than opaque scores.

Its overlap resolver has nested later-result/group scans. Even disjoint inputs
can incur quadratic comparisons. Do not copy that algorithm for the 20,000-span
workload. Specificity/score weighting is a policy choice and is not comparable
to calibrated GLiNER2 probability without evidence.

[Anonymization helper](https://github.com/censgate/redact/blob/6d10a140a8400188704399b1385fb2df89035736/crates/redact-core/src/anonymizers/mod.rs)
sorts spans and appends unchanged slices/replacements. It assumes valid UTF-8
geometry at slicing points; an app adapter must validate all spans before commit,
rather than inheriting unchecked public result structs.

[Workspace manifest](https://github.com/censgate/redact/blob/6d10a140a8400188704399b1385fb2df89035736/Cargo.toml)
pins the optional separate NER crate's `ort` dependency to rc.12. mdoc pins rc.13.
This is a dependency-alignment concern for NER adoption, not a claim that the
pattern-only redact-core crate itself pulls ONNX or cannot resolve.

### anonymize-rs

[Pipeline/deanonymization](https://github.com/qooba/anonymize-rs/blob/a73bff5bfee2df29db19816b62888b000314d80b/anonymize-rs/src/anonymizer/mod.rs)
uses a token-to-original HashMap and repeated global `String::replace` for
restoration. That does not distinguish clicked occurrences, pasted tokens,
shared PERSON markers or editor history.

[FlashText implementation](https://github.com/qooba/anonymize-rs/blob/a73bff5bfee2df29db19816b62888b000314d80b/anonymize-rs/src/anonymizer/flashtext_anonymizer.rs)
passes indices from `char_indices` into a boundary helper using `chars().nth(index)`.
The index is a byte offset but nth expects a character ordinal. An isolated
probe using the exact upstream helper panicked for `"Я "` at byte offset 2,
which is a valid UTF-8 boundary. This verifies that helper defect, not an execution
of the full upstream crate. `replace_keywords` also trims trailing whitespace.
Neither behavior belongs in a Markdown-fidelity path.

[Manifest](https://github.com/qooba/anonymize-rs/blob/a73bff5bfee2df29db19816b62888b000314d80b/anonymize-rs/Cargo.toml)
unconditionally includes actix-web, full tokio, reqwest and tract-onnx. Borrow the
stage separation, retain our approved matcher and runtime, and avoid this package
as an application dependency.

### cloakrs

[Recognizer contract](https://github.com/kadir/cloakrs/blob/765676a6355fecc746105b65376d5aa25fcb7033/crates/cloakrs-core/src/recognizer.rs)
separates scan, locale and validation. [Finding types](https://github.com/kadir/cloakrs/blob/765676a6355fecc746105b65376d5aa25fcb7033/crates/cloakrs-core/src/finding.rs)
name byte-offset semantics explicitly. Useful concepts include a validated score
constructor and stable recognizer identifier; serde-derived deserialization still
needs validation at the app boundary.

[IBAN](https://github.com/kadir/cloakrs/blob/765676a6355fecc746105b65376d5aa25fcb7033/crates/cloakrs-patterns/src/iban.rs)
combines pattern, country length and mod-97 validation;
[cards](https://github.com/kadir/cloakrs/blob/765676a6355fecc746105b65376d5aa25fcb7033/crates/cloakrs-patterns/src/credit_card.rs)
use Luhn. These are candidates for selective reuse with new local fixtures,
not proof that arbitrary matching numbers identify a person.

[Email recognizer](https://github.com/kadir/cloakrs/blob/765676a6355fecc746105b65376d5aa25fcb7033/crates/cloakrs-patterns/src/email.rs)
suppresses matches with URL-like prefixes and certain path/underscore boundaries.
mdoc must inspect hidden Markdown destinations/code/HTML; those suppression rules
cannot be imported unchanged. Keep the full-source contract and source-syntax
protection separate from detector-specific heuristics.

Its sorted overlap deduplication is simpler than nested all-pairs scanning, but
uses length/confidence winner semantics different from mdoc's current policy.
Reuse the data structure technique only while preserving tested outcomes.

### presidio-rs

[Anonymizer engine](https://github.com/jqueguiner/presidio-rs/blob/d3b83a1746a404134a10ad6c2d3d2eda35f523d4/crates/presidio-anonymizer/src/engine.rs)
builds output from left to right and records each replacement's output range using
output length before/after appending. This is particularly useful for immediate
highlight geometry. Its conflict resolver still scans already-kept spans; borrow
the output bookkeeping, not all engine internals.

[Country recognizers](https://github.com/jqueguiner/presidio-rs/blob/d3b83a1746a404134a10ad6c2d3d2eda35f523d4/crates/presidio-analyzer/src/country.rs)
include RU_SNILS with a regex, checksum validator and Cyrillic context term.
[Registry](https://github.com/jqueguiner/presidio-rs/blob/d3b83a1746a404134a10ad6c2d3d2eda35f523d4/crates/presidio-analyzer/src/registry.rs)
registers country recognizers under the `language == "en"` branch. A Russian
recognizer's existence therefore does not establish a working default Russian
pipeline. Check formats, registration, historical/invalid inputs and negatives
before any port; Russian INN needs a separately verified component.

[Analyzer manifest](https://github.com/jqueguiner/presidio-rs/blob/d3b83a1746a404134a10ad6c2d3d2eda35f523d4/crates/presidio-analyzer/Cargo.toml)
has optional ONNX rc.10 dependencies and optional first-use gazetteer downloads.
Do not enable those as part of our local explicit GLiNER2 workflow.

### pii-vault

[Vault](https://github.com/Jiansen/pii-vault/blob/e5fc84c3848c017668ec0149080a10b421b37d51/rust/src/vault.rs)
indexes category/original/context to entries and scopes tokens per vault.
Detokenization loops over entries and globally replaces each token. The actual
token derivation is salted FNV-1a, not a cryptographic guarantee. It serializes
originals; that does not match mdoc's confirmed live-memory-only design.

[Anonymizer results](https://github.com/Jiansen/pii-vault/blob/e5fc84c3848c017668ec0149080a10b421b37d51/rust/src/anonymizer.rs)
include original and replacement strings, but item spans retain input positions
even when replacements change length. mdoc must distinguish source ranges from
post-edit ranges. Repeated `replace_range` calls and per-entry whole-text
restoration are not the recommended dense-document primitives.

## Proposed refinements for the agreed implementation

1. A typed detection boundary: source byte range, existing category, compact
   recognizer/config ID and typed evidence. Do not clone original text or JSON
   explanations into every hit. Rule scores remain heuristic, not model probabilities.
2. A validated batch plan: one captured editor revision, sorted non-overlapping
   edits and explicit post-edit ranges. Validate UTF-8, bounds, exact current
   values, source syntax and replacement policy before modifying anything.
3. A single-pass text builder in the existing generic atomic editor batch path.
   mdoc currently calls `String::replace_range` per edit in reverse order. That
   avoids invalidating input offsets, but repeated suffix movement can be costly.
   Appending unchanged slices and replacements once also derives result spans;
   preserve the same history, caret/selection and Changed semantics.
4. Retain the agreed stable occurrence/provenance IDs and reversible metadata
   journal. External token-to-original maps cannot replace these structures.
5. Use mature matcher/regex primitives directly; implement the small app-owned
   orchestration and lifetime rules ourselves. Copy validators selectively only
   after source attribution, fixture verification and scope approval.

## Confirmed evaluation scope

The user answered "agree" to Q14 on 2026-10-07. An isolated hybrid prototype
and evaluation are authorized. Production selection was deferred until measured results were reviewed; Q15
subsequently selected only email and contextual INN/SNILS.

Q14: should research expand into an isolated hybrid detector prototype before
feature implementation, or should we borrow only architecture/offset techniques?

Recommendation: evaluate a small whitelist of structured recognizers alongside
the existing GLiNER2, outside the product graph. Start with email, phone, Russian
INN/SNILS and selected bank identifiers; use explicit format/context/checksum
validation as appropriate. Compare GLiNER-only, rules-only and combined outputs
with exact UTF-8/category scoring and legal-number/hidden-source/OCR negatives.
Do not adopt a full engine or silently ship new recognizers. Any production
detector change follows measured results and a separate explicit selection.

## Evaluation result and next frontier — 2026-10-07

The isolated evaluation is complete; see the
[retained measurements](../evidence/pseudonymization/2026-10-07-hybrid/measurements.md)
and [reproduction commands](../../tests/fixtures/pseudonymization/hybrid/README.md).
The model ran with network access denied using the app's current FP16 label
families, threshold and tokenizer-bounded windows. At this isolated evaluation checkpoint, no product source, dependency
or behavior had changed.

On 12 new synthetic holdout fixtures (24 expected mentions), the model had
15 exact hits, 19 false positives and 9 misses; the hybrid had 21 hits, 16 false
positives and 3 misses. On the 13 existing short fixtures, both retained 52 hits,
15 false positives and 16 misses. Across all 33 fixtures, selected email/INN/SNILS
rules recover 8 additional exact mentions and remove 5 model false positives.
Individual ablations show email +3 hits, INN +1 and SNILS +4; phone, IBAN and
Russian account/BIC rules added no exact hits. The selected three-rule hybrid
matches full-hybrid counts on every fixture. Zero rule false positives in this
small authored corpus does not establish general precision.

Cached rules scanned exactly 2 MiB with 20,000 matches in a 11.587 ms median
(seven release calls, Apple M4); this excludes model and editor work. Rules
remain conservative and miss table-header contexts, qualifier labels and
damaged identifiers. Gold was not changed to excuse those misses.

Q15 (confirmed): should the agreed implementation include only Unicode/hidden-source
email and explicitly contextual checksum-valid INN/SNILS, alongside experimental
GLiNER2, or remain architecture-only?

Recommendation: include these three small app-owned recognizers with explicit
origin/conflict policy and regression fixtures; defer phone/bank-rule expansion
and full-engine adoption. A checksum rejection must not suppress model candidates.
The user answered "agree". These three app-owned recognizers and the agreed
restoration/refactor are implemented. All detector output remains experimental.
The [implementation report](../evidence/pseudonymization/2026-10-07-restoration/README.md)
records the selected-rule parity checks and editor/lifecycle validation.
