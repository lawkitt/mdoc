# Qualify PII preparation within a bounded product scope

Status: direction accepted, 2026-10-08. The outgoing journey is implemented
experimentally under [ADR 0018](0018-document-local-identity-review.md), with
subsequent review changes in ADRs 0019–0025 and 0032. Production gates remain open.
[ADR 0034](0034-pseudonymization-supported-feature.md) supersedes the rule that
experimental status waits for those gates.

## Decision

The first qualified PII release is a local document-preparation assistant with
measured identification coverage, reliable replacement behavior and explicit
final user review. Removing experimental status requires agreed evidence; it
does not establish that subjects cannot be identified from remaining context.

Qualify English/Russian native and OCR-derived legal text, including PDF/DOCX
conversion and directly opened Markdown, on Apple Silicon macOS and Windows x64.
Keep Linux buildable and state automatic-scanning availability separately.
This describes the required release envelope, not current verified support.

Sanitization covers the complete current Markdown handoff, including hidden
source. Original PDFs/DOCX, filenames, attached files and their metadata remain
outside the sanitization boundary.

Identity consistency is scoped to one live document with explicit variant
linking. Retain the existing live-memory mapping lifetime for this milestone;
matter-wide and persistent mappings remain deferred.

The user confirmed two product journeys: prepare Markdown for external AI
analysis without returning to mdoc, and prepare it for AI analysis with a return
to mdoc. Focus the first production milestone on the journey without a return.
Return import/restoration behavior remains a later design branch; do not silently
extend it to AI drafting or implementation now.

## Rationale

These boundaries support meaningful qualification while avoiding unapproved
artifact-cleaning and mapping-storage workflows. Detection quality, source
integrity, relationship fidelity and runtime/native acceptance need separate
evidence; success in one does not establish the others.

## Open decisions

Numerical release targets, representative holdout quality, correction time,
whole-editor responsiveness and native/platform qualification remain open. Track
them in [production readiness](../design/pseudonymization-production-readiness.md).
The outgoing journey and document-local correction model are settled; AI-response
import and deliberate forgetting of originals remain deferred. No restoration
behavior is removed or newly approved by this ADR.
