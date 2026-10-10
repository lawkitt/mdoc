# Pseudonymization production readiness

Status: qualification gates remain open, 2026-10-10. The outgoing AI-analysis
workflow is implemented experimentally. This note tracks unresolved work;
accepted product scope is in [ADR 0016](../adr/0016-pii-production-qualification-scope.md)
and identity review in [ADR 0018](../adr/0018-document-local-identity-review.md).
[ADR 0022](../adr/0022-remove-anonymization-mode.md) removed Anonymize/shared markers;
subsequent review and popup decisions are in ADRs 0023–0025 and 0032.

## Qualification boundary

Qualify EN/RU legal text from native/OCR PDF, DOCX conversion and directly opened
Markdown on Apple Silicon macOS and Windows x64. The handoff is the complete
current Markdown, including hidden source. Original files, filenames,
attachments and their metadata remain outside replacement scope.

Identity consistency is document-local. Originals and mappings remain live-memory
state; tracked occurrence provenance governs correction and restoration. A copied
alias does not acquire provenance. The first journey ends at external AI handoff;
response import, persistent/matter-wide maps and deliberate forgetting are deferred.

## Open gates

- Agree numerical release targets using measured baselines and a larger holdout;
  the experimental authorization accepted no numerical targets.
- Measure category/boundary/hidden-source misses, false positives, mistaken
  identity joins/splits, wrong ownership and user correction time. Detector
  precision/recall alone cannot qualify relationship-preserving output.
- Qualify native/OCR EN/RU legal documents, ambiguous names/initials/inflections,
  tables, links/code/HTML, damaged identifiers and long documents.
- Verify source integrity, explicit setup/offline operation, cancellation,
  stale results, edits/undo, tab switching, copy/save and provenance lifetime.
- Measure whole-editor responsiveness and resources separately from detector
  runtime and added PII overhead, using fixed viewports and actual scroll positions.
- Complete native keyboard/IME, clipboard, accessibility and platform runtime
  acceptance. Headless success and installation do not establish qualification.

## Existing evidence

These are historical observations, not fresh measurements from this cleanup:

- [Initial qualification](../../tests/fixtures/pseudonymization/README.md): no
  model passed; GLiNER2 was subsequently authorized experimentally.
- [Selected hybrid evaluation](../evidence/pseudonymization/2026-10-07-hybrid/measurements.md):
  87 exact TP, 35 FP and 19 misses across 106 gold mentions in 33 synthetic
  fixtures (71.3% precision, 82.1% recall). RU: 34 TP, 16 FP, 13 misses.
  These precede app syntax filtering/repeat expansion and are not representative
  end-to-end scores. Small organization/address samples each had 50% recall.
- [Restoration measurements](../evidence/pseudonymization/2026-10-07-restoration/README.md):
  roughly 3.4 seconds per dense wrapped hidden-source frame at 2 MiB/20,000
  occurrences, mostly base-editor cost. Efficient tracking does not establish
  whole-editor responsiveness.
- [Identity verification](../evidence/pseudonymization/2026-10-08-identities/README.md)
  and [review UI verification](../evidence/pseudonymization/2026-10-08-ui/README.md)
  cover their stated experimental scope, with native/platform limits retained.

Structured rules require supported same-line context; checksums can reject
damaged but sensitive identifiers. No scan count establishes complete coverage,
and remaining context can still identify subjects. Keep experimental status
until agreed evidence supports a change.
