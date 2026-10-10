# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap tracks work remaining as of 2026-10-10; accepted behavior lives in
[ADRs](docs/adr/), with terminology in [CONTEXT.md](CONTEXT.md).

## Current status

- Local conversion, editable Markdown, PDF/DOCX source previews, multi-file
  opening and full-source Copy Markdown are implemented.
- Explicit offline OCR setup, model Settings and isolated QA comparisons are
  implemented. See [OCR qualification](tests/fixtures/ocr-qualification/README.md)
  and [Settings verification](tests/fixtures/model-settings/README.md) for limitations.
- Document-local pseudonymization includes stable aliases, variant/occurrence
  correction, reviewed ownership, explicit batch Apply, inspectable replacements
  and metadata-aware undo/redo. Pseudonymization is the only PII replacement mode
  ([ADR 0022](docs/adr/0022-remove-anonymization-mode.md)). It is a supported,
  assistive feature ([ADR 0034](docs/adr/0034-pseudonymization-supported-feature.md));
  no model passed the [initial qualification](tests/fixtures/pseudonymization/README.md).
- Offline EN/RU spellcheck with bundled dictionaries is implemented
  ([ADR 0036](docs/adr/0036-bundled-spellcheck.md), [baseline](docs/evidence/spellcheck/README.md));
  its layout cost belongs in step 1's measurements.
- Sidebar, first-use setup, empty-page/file-drop and replacement-popup revisions
  are implemented under ADRs 0026–0028 and 0032. Native/platform acceptance remains
  partial; implementation does not establish qualification.

## Next steps

1. **Measure and resolve responsiveness/resource blockers.** Separate editor
   layout cost from PII overhead using fixed viewports and representative
   workloads. Existing [restoration measurements](docs/evidence/pseudonymization/2026-10-07-restoration/README.md)
   expose a dense-document layout bottleneck; historical timings are not current
   performance guarantees.
2. **Qualify the outgoing AI-analysis journey.** Agree release targets and test
   EN/RU legal/native/OCR text, hidden source, identity joins/splits, ownership and
   correction time on a larger holdout. See [open gates](docs/design/pseudonymization-production-readiness.md).
   These are ongoing quality gates, not a precondition for the supported label.
3. **Complete native and platform acceptance.** Check both themes, keyboard/IME,
   clipboard, accessibility and affected popup, setup and file-drop flows.
   Apple Silicon macOS and Windows x64 are the first qualification targets;
   keep macOS, Windows and Linux buildable. Record unavailable checks explicitly.

Keep the app and internal crates maintainable through concrete ownership/test
improvements ([ADR 0029](docs/adr/0029-codebase-cleanup-charter.md)). Conversion
and OCR algorithm changes belong in the AnyDoc/pdf-inspector forks, backed by
source/output comparisons. Numerical budgets require measured baselines.

## Deferred

- AI-analysis response import and restoration; the initial journey ends at handoff.
- Cross-document identities, persistent/exported replacement maps, deliberate
  forgetting of originals, and stronger anonymization claims.
- Bulk conversion beyond current lazy multi-file opening; design separately.
- Online formatting/OCR repair; provider, credentials, trust and review need decisions.
- Named/collapsible tab groups and opening a dropped folder as a group; folders
  are currently skipped ([ADR 0028](docs/adr/0028-empty-page-and-file-drop.md)).
- Extra recognizers/full PII engines and optional source/page references; qualify
  a concrete workflow before expanding scope.
