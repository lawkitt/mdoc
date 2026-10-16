# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap tracks work remaining as of 2026-10-16; accepted behavior lives in
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
   workloads. The [October 16 stabilization](docs/evidence/stabilization/2026-10-16/results.md)
   addresses case-save failures, refresh/scheduling, spelling, diagnostic/link
   geometry and panel preparation. Whole-document layout cost remains to qualify.
   Ordinary legal documents are the hard interactive target; 2 MiB / 20,000
   mentions form a measured stress tier with bounded work and memory. Set
   numerical budgets after release/native baselines
   ([ADR 0049](docs/adr/0049-measured-stabilization-and-architecture-review.md)).
   Release measurements still expose long-document layout, large case-update
   fan-out and process memory costs; profile these before selecting migrations.
   Existing [restoration measurements](docs/evidence/pseudonymization/2026-10-07-restoration/README.md)
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
4. **Cases and the AI return journey** (accepted design:
   [saved mappings](docs/design/saved-mappings.md), [ADR 0041](docs/adr/0041-cases-and-ai-response-restoration.md)). Save a document's mapping
   as an encrypted local case, group tabs that share one case, and restore AI
   responses into a new tab with reviewable restored values. All four phases are
   implemented, including the Linux Secret Service key, and ship in betas;
   captured ChatGPT/Claude reply fixtures (required before stable 0.1.0) and
   Windows/Linux native checks are still needed.
5. **Preparation conveniences** from the [Anoni review](docs/design/saved-mappings.md#anoni-0810-hands-on-macos-2026-10-11):
   in-app Pseudonymize Clipboard, the before/after view and per-case category
   switches are implemented; a system-wide clipboard hotkey (macOS
   `RegisterEventHotKey`, Windows `RegisterHotKey`) remains. Cases are already
   listed on Home (ADR 0048).
6. **TODO: Broader architectural refactoring**
   ([ADR 0049](docs/adr/0049-measured-stabilization-and-architecture-review.md)).
   After focused stabilization, deliver case coordination and document lifecycle
   as separate, incremental projects: explicit owners for case persistence and
   synchronization, and explicit document transitions across the shell/view.
   GPUI keeps presentation, prompts and scheduling; existing editor/history,
   import and preview owners retain their contracts. Define each migration's
   boundary before implementation. Investigate retained editor layout only if
   remaining release-build costs justify it after local fixes; that replacement
   is not committed work. Preserve behavior and lifecycle invariants throughout.

Keep the app and internal crates maintainable through concrete ownership/test
improvements ([ADR 0029](docs/adr/0029-codebase-cleanup-charter.md)). Conversion
and OCR algorithm changes belong in the AnyDoc/pdf-inspector forks, backed by
source/output comparisons. Numerical budgets require measured baselines.

## Known issues

Found in 0.1.0-beta.11 while recording product media (2026-10-16) on a fictional
letter before claim (lawkitt/web `videos/mdoc/fixture/`). The
[settlement agreement](docs/product-media/Settlement%20agreement.md) reproduces 3
and 4.

1. **OCR loses page structure.** On a clean one-column scan (A4, PP-OCRv6 Small,
   150 DPI) separate paragraphs and headings are merged into one paragraph, and
   table rows split or shift (a cell is promoted to a heading, two rows merge into
   one cell). Rendering the scan at 300 DPI does not help. On the *Brown v. Board*
   page a body line ("even though the physical facilities…") becomes a heading. Layout recovery belongs
   in the AnyDoc/pdf-inspector forks; add the scan to the OCR qualification set.
2. **Highlights stretch across a line wrap.** When a proposed or applied mention
   wraps, its underline or chip box extends to the end of the first line and the
   continuation starts with a stub. Visible in the editor and in Side by side.
3. **Adding part of a detected name splits the identity.** Selecting "Raman" and
   choosing Replace (⌘⌥P) creates a new person alias for every occurrence,
   including the one inside "Priya Raman", whose existing alias is then broken up;
   "Same as…" no longer offers the original identity. A manual addition inside an
   existing mention should leave it intact and offer to link to it.
4. **Surname-only mentions are not linked.** "Ms Raman" (or "Ms Bennett") is not
   proposed as a mention of the detected full name, so it stays in the text after
   Apply. Propose title + surname, possessives and first names as variants of a
   known identity (ADR 0018 review still applies).
5. **Floating controls cover the text being edited.** The "Replace ⌘⌥P" chip sits
   over the line above the selection and the mention bar covers the next lines.
   Place them clear of the selection or the active mention.

## Deferred

- An optional case passphrase on top of the OS keyring
  ([ADR 0045](docs/adr/0045-prompt-free-case-key.md)).
- Exporting or sharing a case key (encrypted file plus passphrase), a
  cross-case dictionary, deliberate forgetting of originals, and stronger
  anonymization claims. Saved cases and restoration moved to step 4.
- Format-preserving pseudonymized DOCX/PDF export; an audit journal and
  compliance report of replacements; a re-identification risk hint.
- Bulk conversion beyond current lazy multi-file opening; design separately.
- Online formatting/OCR repair; provider, credentials, trust and review need decisions.

Not planned: realistic fake-name pseudonyms (harder to restore, may collide with
real names; [ADR 0018](docs/adr/0018-document-local-identity-review.md)),
an irreversible redaction mode ([ADR 0022](docs/adr/0022-remove-anonymization-mode.md)),
watermarking and watched folders (outside AI preparation).
- Extra recognizers/full PII engines and optional source/page references; qualify
  a concrete workflow before expanding scope.
