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
7. **Reduce Rust build-cache growth.** The main `target/` directory reached
   approximately 187 GiB on 2026-10-16. Measure disk growth, cold builds and warm
   rebuilds for representative development and test workflows before changing
   settings. Evaluate `debug = "line-tables-only"` with incremental compilation
   retained; document the loss of debugger variable inspection and how to request
   full debug information. Keep the pinned toolchain, features and compiler flags
   consistent across build entry points, and document cleanup of build output
   when temporary worktrees are retired. Evaluate `sccache` separately for shared
   dependency builds, accounting for its own disk use and incremental/linking
   limitations. Record measured savings and rebuild-time tradeoffs; preserve
   release settings and avoid automatic eviction or arbitrary cache caps.

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

## UI/UX backlog (2026-10-16)

Future work from the entity-review and workspace screenshots; these items are
pending fixes and refinements, not completed behavior.

1. **Center mentions selected in the Replacements panel.** Clicking an entity
   or mention row should reveal, highlight and focus the corresponding mention
   near the vertical center of the Markdown editor's visible viewport. Currently
   it can remain against the top edge, requiring manual scrolling. Account for
   the mention popup and clamp scrolling at document boundaries so the selected
   text stays readable and clear of floating controls.
2. **Make panel-to-editor navigation smooth and prompt.** Animate the scroll to
   the selected mention with a short transition. Rapid successive selections
   should move toward the latest target without queued animations or jumps to
   stale targets; manual scrolling should interrupt the transition. Respect
   Reduce Motion with immediate positioning.
3. **Eliminate mention-popup flicker.** Selecting entities or stepping through
   their mentions in the panel should show one stable popup anchored to the
   active mention, including while the editor scrolls. Avoid transient dismissal,
   duplicate popups and intermediate frames at an incorrect position.
4. **Make the mention popup compact and reversible.** Reduce excess width,
   padding and spacing while keeping the original, alias, category, Replace
   checkbox and scope controls readable. Default to the entire selected entity
   group, with a clear toggle for this mention only. Unchecking Replace must keep
   the popup open so it can be checked again immediately. After dismissal, kept
   mentions should retain a subtle underline and remain clickable to reopen the
   controls and re-enable replacement. Preserve occurrence identity and the chosen
   scope so re-enabling one mention leaves other kept mentions unchanged; marking
   intent must retain the existing explicit Apply behavior.
5. **Separate the mention popup visually from the editor.** Give it a restrained
   but sufficiently contrasting border in both light and dark themes. Verify the
   open popup over real document text in each theme so its edge remains distinct
   without a heavy outline.
6. **Support the editor font-increase shortcut reliably.** Make Cmd + + on macOS
   and Ctrl + + on Windows/Linux increase Markdown text size through the same
   action as the existing zoom button. ADR 0048 already documents Cmd + = / − / 0;
   verify action routing and accept the plus key, including Shift where required
   by the keyboard layout, alongside the existing equals-key binding. Retain the
   current zoom steps and limits.
7. **Add expand/collapse-all controls to the Replacements panel.** Provide a
   compact icon control to expand all entity groups or collapse them together,
   with a clear tooltip and accessible label for the available action. Expanding
   or collapsing groups must preserve the current selection and replacement
   decisions.
8. **Make the notification popup easy to dismiss.** Clicking the bell while its
   popup is open should close it. Add a close button inside the popup using the
   same cross icon, size and button styling as other dismissible UI. Closing the
   popup should preserve notices and their attention state.
9. **Dismiss Settings by clicking outside.** Clicking the surrounding area
   outside the Settings window should close it consistently with other overlays;
   clicks inside Settings must continue to work without dismissing it.
10. **Add workspace Back and Forward navigation.** Place left-arrow and
    right-arrow icon buttons immediately to the right of the sidebar
    expand/collapse button, before the Home label or document title. Keep them
    available with the sidebar expanded or collapsed. Navigate through visited
    documents and Home, restore the destination's existing view state, disable
    unavailable directions, skip closed documents and clear forward history when
    a new destination is opened after going back. Use consistent icon sizes,
    tooltips and accessible labels.

Validate these interactions in the live editable UI in both themes, including
long documents, wrapped mentions, rapid panel selection, kept mentions and
keyboard navigation; static previews alone do not establish acceptance.

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
