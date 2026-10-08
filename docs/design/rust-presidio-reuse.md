# Rust Presidio reuse research — 2026-10-08

Status: research complete; selected structured adapter implemented experimentally
after the user's "I agree. Implement it" on 2026-10-08.
Requested during the pseudonymization production-readiness grilling. Q7–Q11
remain unanswered; this research does not confirm their recommendations.

## Finding

Rust implementations of Presidio exist. The most direct candidate inspected is
`jqueguiner/presidio-rs`, publishing `presidio-analyzer` and `presidio-anonymizer`
0.1.11 under MIT. It offers reusable recognizers, checksum validators, context
enhancement, a registry, a pluggable NLP interface and replacement operators.
It does not provide mdoc's proposed editable identity/variant/ownership model.

Reuse detector components through a narrow adapter before writing more generic
recognizer infrastructure. Keep existing Markdown/source protection, occurrence
provenance, editor transactions, user decisions and correction UX in mdoc.
The important remaining gap is identity association, not numbered token generation.

## Method and limits

- Rechecked upstream GitHub source and crates.io metadata; shallow-cloned eight
  repositories into `/tmp/mdoc-pii-oss-20261008.Cfk56N` and downloaded the published
  `supportfile_pii_anonymizer` 0.1.14 and `dbmcp-pii` 0.13.2 source packages.
- Inspected manifests, licenses, recognizer registration, NLP interfaces, mapping
  and replacement implementations. Prior 2026-10-07 research was a starting point;
  relevant claims were refreshed, not assumed current.
- Compiled and ran a synthetic Rust API probe against the inspected Presidio and
  cloakrs source. It tests configuration, Unicode geometry and exact-value mapping;
  it deliberately supplies prelocated person spans, so it measures no name recall.
- Ran three isolated Cargo resolution probes with mdoc's exact `ort` rc.13 and
  `gliner2-rs` 0.9.6 pins. These are minimal dependency graphs, not a full mdoc build,
  runtime coexistence test or platform qualification.
- No model weights downloaded, no neural inference executed, no upstream full
  suite run, no EN/RU legal/OCR accuracy comparison or speed benchmark performed.

## Candidates and exact inspected revisions

| Project | Source revision / version | Published version checked | License | Assessment |
| --- | --- | --- | --- | --- |
| [jqueguiner/presidio-rs](https://github.com/jqueguiner/presidio-rs) | `d3b83a1746a404134a10ad6c2d3d2eda35f523d4`, 0.1.11 | analyzer/anonymizer 0.1.11 | MIT | First candidate for reusable rule detection; mapping/identity UX still belongs to mdoc. |
| [kadir/cloakrs](https://github.com/kadir/cloakrs) | `765676a6355fecc746105b65376d5aa25fcb7033`, workspace 0.4.1 | cloakrs-core 0.4.0 | MIT | Useful recognizer primitives and exact-repeat prompt mapping; no Russian identity resolver. Source and published versions differ. |
| [censgate/redact](https://github.com/censgate/redact) | `6d10a140a8400188704399b1385fb2df89035736`, 0.12.5 | redact-core 0.12.5 | Apache-2.0 | Reusable structured detection; larger policy/crypto surface. Separate NER pins ort rc.12. |
| [DataFog/datafog-core](https://github.com/DataFog/datafog-core) | `1299fd6e53f177a6cb5ec29b231bb24dc3d191dc`, 0.4.2 | 0.4.2 | MIT | Explicit Unicode ranges and transform records; keyed value pseudonyms, not readable identity aliases. |
| [haymon-ai/dbmcp](https://github.com/haymon-ai/dbmcp) | `69fa44024f451a479f1d770d2de97c97d80fecd3` plus published dbmcp-pii source | dbmcp-pii 0.13.2 | MIT | Rules/validators useful; package graph and unconditional ONNX dependency make direct adoption unattractive. |
| [supportfile_pii_anonymizer](https://docs.rs/crate/supportfile_pii_anonymizer/0.1.14) | published source package 0.1.14 | 0.1.14 | MIT | Deterministic diagnostic-data pseudonyms for hosts/IPs/Azure IDs; not a legal-name solution. |
| [IronCoreLabs/rs-presidio](https://github.com/IronCoreLabs/rs-presidio) | `7647b39d281e8ad32fbd727912d07d3c2c952296`, workspace 0.1.0 | not checked as a distinct published distribution | MIT | Maintainer describes it as a rescue snapshot without maintenance/bugfix guarantees. Do not choose as the production foundation. |
| [Jiansen/pii-vault](https://github.com/Jiansen/pii-vault) | `e5fc84c3848c017668ec0149080a10b421b37d51`, source 0.3.0 | 0.2.0 | MIT | Per-vault exact-value/context indexing, serializable originals and global restoration. No semantic identity resolver; mismatches current mapping lifetime. |

License entries describe checked source declarations/files, not a training-data
audit or approval to redistribute a selected model. Preserve applicable upstream
notices if adopting dependencies or copying code.

## Presidio port: useful components and constraints

Useful source:

- [Registry](https://github.com/jqueguiner/presidio-rs/blob/d3b83a1746a404134a10ad6c2d3d2eda35f523d4/crates/presidio-analyzer/src/registry.rs)
  and [recognizers](https://github.com/jqueguiner/presidio-rs/blob/d3b83a1746a404134a10ad6c2d3d2eda35f523d4/crates/presidio-analyzer/src/recognizer.rs)
  can provide pattern/context/validation orchestration.
- [Country validators](https://github.com/jqueguiner/presidio-rs/blob/d3b83a1746a404134a10ad6c2d3d2eda35f523d4/crates/presidio-analyzer/src/country.rs)
  include Russian SNILS. No INN recognizer was found in the inspected analyzer.
- [NLP interface](https://github.com/jqueguiner/presidio-rs/blob/d3b83a1746a404134a10ad6c2d3d2eda35f523d4/crates/presidio-analyzer/src/nlp.rs)
  is pluggable. Its default engine tokenizes and lowercases; it performs no NER
  and does not perform Russian morphological lemmatization or coreference.
- [Anonymizer](https://github.com/jqueguiner/presidio-rs/blob/d3b83a1746a404134a10ad6c2d3d2eda35f523d4/crates/presidio-anonymizer/src/engine.rs)
  produces output ranges, but its default replace operator emits `<PERSON>` for
  every person. Built-in deterministic surrogates key off source text, not a
  resolved identity, and are not guaranteed unique across distinct originals.

Configuration nuance, verified by execution:

- `RecognizerRegistry::with_predefined("ru")` registers zero built-ins when
  optional gazetteers are disabled, because registration is inside an EN branch.
- `AnalyzerEngine::new()` loads the EN registry, whose pattern recognizers are
  language-independent. Calling this engine with `language="ru"` detected our
  synthetic SNILS and email with valid UTF-8 source ranges. Therefore "no Russian
  recognition" would be an incorrect blanket description of this package.
- A real EN/RU adapter needs an explicit recognizer whitelist/registration policy
  and context tests, including native/OCR identifiers. A Cyrillic context term or
  language-agnostic regex does not qualify Russian person/organization detection.

Integration constraints:

- Rules-only analyzer successfully resolves alongside ort rc.13 + GLiNER2 0.9.6.
- Enabling `onnx-pos` requires exact ort rc.10 and failed isolated resolution with
  mdoc's rc.13. Keep this feature disabled; reuse existing detector outputs rather
  than replacing the approved NER runtime.
- Gazetteers are optional and may download on first use. They are outside the
  app's explicit pinned setup contract unless separately adapted and approved.
- Public anonymizer spans are sliced without validating UTF-8 boundaries. The
  probe passed start=1/end=2 for `"Я"` and observed a panic. Any use requires mdoc's
  existing validated boundary; the existence of an upstream engine is insufficient.
- Package metadata points at `data-privacy-stack/presidio-rust`, which returned
  404 to both web access and git clone during this research. The inspected source
  remains available in jqueguiner's repository. Resolve provenance/maintenance
  responsibility before making it an important production dependency.

## Mapping alternatives do not solve identity association

[cloakrs prompt mapping](https://github.com/kadir/cloakrs/blob/765676a6355fecc746105b65376d5aa25fcb7033/crates/cloakrs-core/src/prompt.rs)
uses `(entity type, exact original text)` as the key. Our synthetic prelocated
mentions produced:

```text
Input:  😀 Анна Иванова / Анны Ивановой / Анна Иванова
Output: 😀 [PERSON_1] / [PERSON_2] / [PERSON_1]
```

Exact repeats reuse aliases and strict restoration round-trips Unicode. The
inflected variant stays separate. Mapping entries contain one original span per
unique value, not all occurrence assignments or reversible editor-history deltas.
Its existing bracket/brace token formatting also differs from mdoc's approved
syntax-safe plain tokens. The bundled person-name heuristic uses a Latin-letter
pattern and dictionaries; custom recognizers can supply other findings, but
that extension seam is not Russian legal-name coverage. The email recognizer
suppresses some URL contexts, contrary to mdoc's whole-hidden-source scope.

[DataFog's contract](https://github.com/DataFog/datafog-core/blob/1299fd6e53f177a6cb5ec29b231bb24dc3d191dc/docs/adr/001-privacy-core-contract.md)
explicitly says value pseudonymization is not identity resolution. It uses
HMAC-SHA-256 of exact UTF-8 values, rendered as 44-character Base64. No case folding,
normalization or semantic canonicalization occurs. The same key/value is stable;
different name forms stay different. Names are discovered from structured JSON
name fields, not general Markdown NER. RU is not an accepted explicit locale in
the inspected capability table. Its range/transform contracts are useful references,
but the token and model scope do not deliver our proposed identity workflow.

The published dbmcp-pii 0.13.2 package unconditionally depends on ort rc.10 with
download-binaries; repository HEAD uses rc.12. Published-source resolution with
mdoc's rc.13 failed. Its replacement operators do not provide semantic identity
grouping. Prefer selected validators/source techniques over this package graph.

## A separate Rust coreference candidate

[SergioArrighi/corpipe-rs](https://github.com/SergioArrighi/corpipe-rs) is a Rust
adaptation of UFAL CorPipe that predicts mentions and assigns entity IDs. This
targets same-identity association more directly than the PII libraries above.
Inspected HEAD: `9670934b054d0762daee35907e934bee5850674d`, source version 0.3.0;
crates.io currently publishes 0.2.1. Code license is MPL-2.0.

- Source HEAD supports CorPipe 25/26 two-stage base and CorPipe 26 two-stage large,
  using Candle and UDPipe; it does not support the 26 one-stage architecture.
- Author-reported short-input release footprints: base checkpoint 1.11 GiB,
  peak RSS 2.52 GiB; large checkpoint 2.21 GiB, peak RSS 4.71 GiB. These are the
  author's measurements, not a local mdoc benchmark or long-document bound.
- Default inference limits include 1 MiB source, 8,192 tokens and 256 sentences,
  which differ from mdoc's scan envelope. Results use token mention spans, requiring
  a verified mapping to immutable Markdown byte offsets.
- [UFAL's released weights](https://github.com/ufal/crac2026-corpipe#the-released-models)
  are CC BY-NC-SA 4.0, separately from the source license. Their non-commercial
  condition blocks assuming ordinary commercial-product suitability. A suitable
  licensed model/permission would be needed before considering that use.
- Upstream training lists EN datasets and `ru_rucor`; this does not establish
  Rust-port EN/RU legal/OCR accuracy, source fidelity or macOS/Windows operation.

Keep as a research lead, not an adoption recommendation. No weights were fetched
and no coreference inference was run. No inspected Presidio-style library provides
qualified name-variant grouping, ownership relations and editable mapping UX.
This does not claim that no such component can exist elsewhere.

## Recommendation for mdoc

1. Evaluate the Presidio analyzer's selected rule recognizers as an isolated
   detector adapter, with all network/extra-NLP features disabled. Compare against
   current GLiNER2 + email/INN/SNILS behavior on the same frozen input. Add no broad
   entities merely because they are available: ordinary dates/amounts/legal refs
   remain outside the current automatic replacement contract.
2. Keep the existing detector pin and runtime while evaluating structured reuse.
   Accept results only through app-owned UTF-8/source/category/evidence checks.
3. Keep identity assignments, explicit variant decisions, ownership policy if
   selected, stable alias allocation and correction/undo in the app. These are
   product contracts rather than a reason to rebuild generic recognizer engines.
4. Do not replace the current editor commit/provenance path with a standalone
   string anonymizer; it already handles different originals sharing markers,
   revisions, cancellation, source syntax and undo.
5. Before writing a sophisticated identity resolver, separately qualify reusable
   morphological/coreference components with compatible weights/licenses/resources.
   Coreference, person-company affiliation and contact ownership are distinct tasks.

No detector is approved for production by this research. The app's proposed
identity model, automation policy and UI remain interview decisions.

## Implemented reuse boundary

The user approved this recommendation and requested implementation on 2026-10-08.
The application now pins the published `presidio-analyzer =0.1.11` package with
default features disabled; `Cargo.lock` retains its crates.io checksum
`b1420e37c6047e45160308046a01200f1d1fb44d0c7d56e119a5458d9d7deff3`.
No gazetteer, network setup or optional ONNX/POS feature is enabled. The existing
`ort =2.0.0-rc.13` and `gliner2-rs =0.9.6` pins are retained. The package does
unconditionally depend on `phonenumber-rs`, although mdoc never invokes its phone
recognizer; this transitive dependency is present in the lock and license report.

`src/pseudonymization_detector/structured.rs` is the adapter. It configures only
email, INN and SNILS as Presidio `PatternRecognizer` definitions, and reads their
compiled patterns and validators directly. It does not call the full engine or
`PatternRecognizer::analyze`: that method creates per-hit explanations and uses
quadratic `remove_contained` in the pinned version. mdoc's existing ingestion
already owns evidence ordering and overlap policy. Direct regex matches yield
UTF-8 byte spans in the exact immutable source; no normalized text, arbitrary
external offsets or string anonymizer enter the editor commit path.

The deliberately narrow reuse preserves current behavior rather than claiming
new recall:

- Email starts from the upstream definition, with mdoc's Unicode-aware pattern
  retained. The upstream ASCII pattern would partially match Cyrillic addresses.
- SNILS starts from the upstream country definition and uses its checksum
  validator. mdoc retains supported contiguous/hyphenated/tab formats, strict
  same-line labels and digit boundaries, and rejection of all-zero values.
- INN uses the library's pattern/validator configuration but retains the existing
  local checksum, because the pinned upstream has no INN recognizer.
- Rule evidence remains `Email`/`Inn`/`Snils` with mdoc's 0.9/0.95 heuristic
  scores. Upstream checksum promotion to 1.0 is not treated as model probability.
- Rules still run only after successful GLiNER2 inference. Existing source bounds,
  checkpoints, stale-result guards, syntax filtering, review and restoration/undo
  contracts continue to apply. No standalone rules-only automatic fallback is added.

The MIT notice is preserved in `resources/presidio-license.txt` and `NOTICE.md`;
the complete dependency license report is regenerated. The 33-fixture selected-rule
comparison is retained as an exact regression gate, with added checks for strict
scope/context and a 2 MiB / 20,001-occurrence Unicode workload. Verification results
are recorded below after execution. These checks validate this adapter change, not
the EN/RU detector, semantic identity grouping or Windows runtime.

### Implementation verification — Apple Silicon macOS

- `cargo check --locked -p mdoc --bin mdoc`: passed.
- `cargo test --locked -p mdoc --bin mdoc pseudonymization_detector::structured`:
  all five checks passed, including exact selected-rule predictions on all 33
  frozen fixtures, Unicode hidden-source spans, model non-veto, strict scope and
  the dense source workload.
- `cargo test --locked --workspace`: 418 passed, 15 ignored across 12 suites.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all --check` and `git diff --check`: passed.
- `cargo about generate about.hbs -o THIRD-PARTY-LICENSES.html`: passed.
  Published Presidio source omits its root license file, so the upstream copyright
  notice is explicitly retained in the report template as well as the complete
  preserved license in `resources/presidio-license.txt`.
- Resolved package features/manifest metadata confirm no optional Presidio
  features and the unchanged GLiNER2 0.9.6 / ONNX Runtime rc.13 pins.
- Dense adapter observation: 2,097,152 bytes / 20,001 occurrences scanned in
  151.7 ms in one unoptimized test run. Every source slice/category was checked.
  This is an adapter observation, not inference/layout timing, a release benchmark
  or a latency guarantee; there is no timing assertion in the regression test.

- `cargo build --release --locked -j 1`: passed. Existing Cargo publish-age and
  `block` future-incompatibility advisories remain; no build failure.
- `cargo test --locked -p mdoc --bin mdoc experimental_model_offline_scan --
  --ignored --nocapture`: passed in 11.18 seconds using the preverified FP16 model
  cache and installed native runtime, without downloads. The multi-window hybrid
  scan returned valid source spans through the later source, and active/pre-set
  cancellation rejected partial results. This smoke test establishes local runtime
  coexistence, not model accuracy or production qualification.

Windows/Linux execution and native UI acceptance were not rerun for this adapter.

## Retained execution evidence

[Probe results](../../tests/fixtures/pseudonymization/results/2026-10-08-oss/probe-results.json),
[probe source](../../tests/fixtures/pseudonymization/results/2026-10-08-oss/probe-main.rs),
[resolution results](../../tests/fixtures/pseudonymization/results/2026-10-08-oss/compatibility-results.json)
and adjacent manifests/lock/logs retain the checks. Reproduce by cloning the pinned
Presidio/cloakrs source into sibling directories named `presidio-rs` and `cloakrs`,
placing the probe manifest at `probe/Cargo.toml`, source at `probe/src/main.rs`,
then running `cargo run --manifest-path probe/Cargo.toml --locked -j 2`.
Resolution manifests similarly expect a `compatibility/<variant>` directory and
the pinned sibling sources or downloaded published package.
