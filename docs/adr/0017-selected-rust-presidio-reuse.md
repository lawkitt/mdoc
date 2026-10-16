# Reuse selected Rust Presidio components through mdoc's detector adapter

Status: Accepted, 2026-10-08. Extended by
[ADR 0018](0018-document-local-identity-review.md) (labelled ОГРН/ОГРНИП).

## Decision

Pin MIT-licensed `presidio-analyzer` 0.1.11 with default features off.
Configure only email and contextual checksum-valid INN/SNILS in
`pseudonymization_detector::structured`; GLiNER2 and the shared ONNX Runtime
stay.

Reuse the library's pattern/validator configuration and SNILS checksum. Keep
mdoc's Unicode email and identifier patterns, strict same-line context and digit
boundaries, zero rejection, local INN checksum and heuristic scores. Matches are
exact UTF-8 byte spans of the immutable Markdown; ingestion, syntax protection,
freshness, review decisions, editor commits and provenance stay app-owned.

Do not call the full analyzer, anonymizer or `PatternRecognizer::analyze` (the
latter removes containment quadratically; mdoc already resolves overlaps). No
gazetteers, optional ONNX/POS features, extra categories or rules-only fallback.

## Consequences

The upstream ASCII email pattern and permissive SNILS defaults would regress
behavior, so app policy stays explicit; INN has no upstream recognizer. The
package's phone-library dependency is retained and attributed, though unused.
Frozen corpus predictions and Unicode/context/dense-source checks gate changes.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0017-selected-rust-presidio-reuse.md),
[source research](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/rust-presidio-reuse.md).
