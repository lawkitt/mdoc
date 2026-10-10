# Reuse selected Rust Presidio components through mdoc's detector adapter

Status: accepted and implemented experimentally, 2026-10-08. The user approved
the Rust Presidio research recommendation and requested implementation.

## Decision

Pin the published MIT-licensed `presidio-analyzer` 0.1.11 package, with default
features disabled. Configure only email and contextual checksum-valid INN/SNILS
in `pseudonymization_detector::structured`; keep GLiNER2 and the shared ONNX Runtime.

Reuse the library's pattern/validator configuration and SNILS checksum validator.
Retain mdoc's Unicode email and identifier format patterns, strict same-line
context/digit boundaries, zero rejection, local INN checksum, and heuristic evidence
scores. Direct matches refer to exact immutable Markdown UTF-8 byte spans. Existing
ingestion, syntax protection, source freshness, review decisions, editor commits
and occurrence provenance remain the app-owned acceptance/replacement boundary.

Do not invoke the full analyzer, anonymizer or `PatternRecognizer::analyze`; the
last uses quadratic containment removal in the pinned version, while mdoc already
resolves overlaps. Do not enable gazetteers or optional ONNX/POS features, broaden
entity categories or add a rules-only automatic fallback as part of this adoption.

## Rationale and consequences

Selected reuse establishes a maintained dependency seam without replacing the
reviewable document-editing contract. The upstream ASCII email pattern and permissive
SNILS defaults would regress existing behavior, so app policy remains explicit.
The absence of an upstream INN recognizer still requires the local checksum.
The package's unconditional phone-library dependency is retained and attributed,
although its recognizer is not invoked.

Frozen selected-rule corpus predictions and Unicode/context/dense-source checks
gate this change. Production detector accuracy, semantic identity associations,
the identity correction UI and platform qualification remain separate open work.
This decision does not remove experimental status.

See [historical source research, implementation boundary and verification](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/rust-presidio-reuse.md)
and [production-scope ADR](0016-pii-production-qualification-scope.md).
