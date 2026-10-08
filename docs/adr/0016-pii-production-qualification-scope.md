# Qualify PII preparation within a bounded product scope

Status: direction accepted, 2026-10-08. Detailed production gates remain open;
the first-journey product workflow is implemented under ADR 0018. The selected Rust Presidio structured-adapter
reuse was separately authorized on 2026-10-08. First-journey identity review was
subsequently authorized and implemented under [ADR 0018](0018-document-local-identity-review.md);
production qualification gates remain open.

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

The user agreed to Q1–Q4 and requested more product brainstorming, describing
Anonymize as identifier removal without restoration for external AI use.
Clarify whether that means an ordinary one-way journey or deliberate forgetting
of originals. The first Pseudonymize journey is now settled; identity grouping,
automatic association policy and mapping correction UX precede technical gates.
No restoration behavior has been removed or newly approved by this ADR.

See the [ongoing design tree](../design/pseudonymization-production-readiness.md).
ADRs 0002, 0013 and 0014 continue to govern implemented behavior until amended.
