# Qualify PII preparation within a bounded product scope

Status: Accepted, 2026-10-08. Superseded in part by
[ADR 0034](0034-pseudonymization-supported-feature.md) (no experimental label
while gates are open) and [ADR 0041](0041-cases-and-ai-response-restoration.md)
(return journey and saved mappings).

## Decision

The PII feature is a local document-preparation assistant: measured
identification coverage, reliable replacement and explicit final user review.
No evidence establishes that subjects cannot be identified from remaining
context.

- **Envelope**: English and Russian native and OCR-derived legal text,
  including PDF/DOCX conversion and directly opened Markdown, on Apple Silicon
  macOS and Windows x64. Linux stays buildable, with automatic-scanning
  availability stated separately.
- **Boundary**: the complete current Markdown handoff, including hidden source.
  Original PDFs/DOCX, filenames, attachments and their metadata are outside it.
- **Identity**: consistency is scoped to one document, with explicit variant
  linking (cases extend this, ADR 0041).
- **Journeys**: prepare Markdown for external AI analysis (first), and bring the
  reply back (ADR 0041). AI drafting is not in scope.

## Rationale

Detection quality, source integrity, relationship fidelity and runtime/native
acceptance need separate evidence; success in one does not establish the
others. The open qualification gates are ROADMAP work.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/5cad017e5a38efe1f009d47f54d1f0d9e2995547/docs/adr/0016-pii-production-qualification-scope.md),
[last gate review](https://github.com/lawkitt/mdoc/blob/5cad017e5a38efe1f009d47f54d1f0d9e2995547/docs/design/pseudonymization-production-readiness.md).
