# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap tracks work remaining as of 2026-10-16 (latest: 0.1.0-beta.11);
accepted behavior lives in [ADRs](docs/adr/), with terminology in
[CONTEXT.md](CONTEXT.md). Structure and stable scope:
[ADR 0051](docs/adr/0051-roadmap-structure-and-stable-scope.md).

## Current status

Local conversion with PDF/DOCX previews, offline OCR, assistive pseudonymization
([ADR 0034](docs/adr/0034-pseudonymization-supported-feature.md)), encrypted
cases with AI-reply restoration ([ADR 0041](docs/adr/0041-cases-and-ai-response-restoration.md)),
EN/RU spellcheck and the redesigned workspace shell ([ADR 0048](docs/adr/0048-workspace-shell-redesign.md))
ship in betas. Implementation does not establish qualification: native/platform
acceptance is partial and no PII model passed the [initial qualification](tests/fixtures/pseudonymization/README.md).

## 0.1.0 stable

Stable waits only on **must-fix** items. Should-fix items ship if ready;
otherwise they move to the top of [After 0.1.0](#after-010). Items are in
priority order.

### Must-fix

1. **[PII] Propose shorter and inflected forms of a known identity.**
   *Found in beta.11 (known issue 4).* "Ms Raman" or "Ms Bennett" is not proposed
   as a mention of the detected full name and stays in the text after Apply.
   *Done when:* title + surname, possessives, surname alone and RU inflections
   are proposed as [variants](CONTEXT.md) for review; a first name alone is not.
   Record the change to [ADR 0018](docs/adr/0018-document-local-identity-review.md)
   in a new ADR.
2. **[Perf] Ordinary legal documents are responsive.** Separate editor layout cost
   from PII overhead using fixed viewports. The [October 16 stabilization](docs/evidence/stabilization/2026-10-16/results.md)
   fixed case saves, refresh scheduling, spelling, diagnostic/link geometry and
   panel preparation; whole-document layout cost (including
   [spellcheck](docs/evidence/spellcheck/README.md) layout), case-update fan-out and process
   memory remain. Debug `just perf` still fails ordinary host scroll and DOCX
   preview p95 (16.76 vs 16.667 ms). Profile before choosing migrations
   ([ADR 0049](docs/adr/0049-measured-stabilization-and-architecture-review.md)).
   *Done when:* on a release build on the M4 reference Mac, the ordinary scroll,
   typing and navigation workloads and the public DOCX/PDF preview workloads reach
   p95 frame ≤ 16.7 ms, confirmed by a native spot-check on a real ~30-page
   contract. Debug ceilings stay regression guards.
3. **[Cases] Captured AI-reply fixtures for Restore.** Cases and restoration
   ([saved mappings](docs/design/saved-mappings.md)) are implemented in all four
   phases, including the Linux Secret Service key.
   *Done when:* four replies to the settlement-agreement demo (ChatGPT and Claude,
   each EN and RU) live under `tests/fixtures/`, at least one with Markdown tables
   and paraphrased or inflected aliases, and Restore leaves no alias unrestored
   in any of them.
4. **[Platform] Native and platform acceptance** on Apple Silicon macOS and
   Windows x64; keep Linux buildable. Done last, because it checks the finished UI.
   *Done when:* one checklist per platform at `docs/evidence/platform/<date>/README.md`
   covers both themes, IME (RU plus a CJK smoke test), clipboard,
   VoiceOver/Narrator basics, and the popup, setup and file-drop flows, and lists
   unavailable checks explicitly. The maintainer runs the Windows checks.

### Should-fix

1. **[UI] Mention popup: stable, compact, reversible, bordered and clear of the
   text.** *From beta.11 known issue 5 and the 2026-10-16 UI backlog.*
   - One stable popup anchored to the active mention while selecting entities,
     stepping through mentions or scrolling: no transient dismissal, duplicates
     or frames at a wrong position.
   - Less width, padding and spacing; original, alias, category, Replace and scope
     controls stay readable. Default scope is the whole entity group, with a clear
     "this mention only" toggle.
   - Unchecking Replace keeps the popup open. After dismissal, kept mentions keep
     a subtle underline and reopen the controls on click. Occurrence identity and
     chosen scope are preserved, and explicit Apply is unchanged.
   - A restrained border that stays distinct over real text in both themes.
   - The "Replace ⌘⌥P" chip and the mention bar no longer cover the selection or
     the lines around it.

   *Done when:* verified in the live editor in both themes on a long document
   with wrapped and kept mentions and rapid panel selection.
2. **[UI] Highlights follow line wraps.** *Known issue 2.* A wrapped proposed or
   applied mention extends its underline or chip box to the end of the first line,
   and the continuation starts with a stub.
   *Done when:* each wrapped segment is boxed separately, in the editor and in
   Side by side.
3. **[UI] Small shell fixes.**
   - Cmd + + (macOS) and Ctrl + + (Windows/Linux) increase Markdown text size
     through the zoom button's action, accepting the plus key (with Shift where the
     layout requires) alongside Cmd + = from ADR 0048; steps and limits unchanged.
   - Clicking the bell while its popup is open closes it; the popup has a close
     button styled like other dismissible UI. Notices keep their attention state.
   - Clicking outside Settings closes it, like other overlays; clicks inside do not.

   *Done when:* each works with the keyboard and mouse on macOS and Windows.

## After 0.1.0

In priority order, after any carried-over should-fix items.

1. **[Perf] Stress tier and numerical budgets.** The 2 MiB / 20,000-mention tier
   has bounded work and memory; set numerical budgets from release/native
   baselines. [Restoration measurements](docs/evidence/pseudonymization/2026-10-07-restoration/README.md)
   are historical, not guarantees.
2. **[UI] Center and smoothly scroll to the selected mention.** Clicking an
   entity or mention row in the Replacements panel reveals, highlights and focuses
   the mention near the vertical center of the editor, clear of the popup and
   clamped at document edges. Animate briefly; rapid selections head for the
   latest target without queued animations; manual scrolling interrupts; Reduce
   Motion positions immediately.
3. **[UI] Expand/collapse all in the Replacements panel.** A compact icon
   control with tooltip and accessible label, preserving selection and
   replacement decisions.
4. **[UI] Workspace Back/Forward.** Arrow buttons right of the sidebar toggle,
   available with the sidebar expanded or collapsed. Navigate visited documents
   and Home, restore view state, disable unavailable directions, skip closed
   documents, clear forward history on new navigation.
5. **[OCR] Recover page structure.** *Known issue 1.* On a clean one-column A4
   scan (PP-OCRv6 Small, 150 DPI; 300 DPI does not help) paragraphs and headings
   merge, and table rows split or shift; on the *Brown v. Board* page a body line
   becomes a heading. Fix in the AnyDoc/pdf-inspector forks. Add the scan to the
   [OCR qualification set](tests/fixtures/ocr-qualification/README.md) now.
6. **[Cases] System-wide Pseudonymize Clipboard hotkey** (macOS
   `RegisterEventHotKey`, Windows `RegisterHotKey`). The in-app action, before/after
   view and per-case category switches from the [Anoni review](docs/design/saved-mappings.md#anoni-0810-hands-on-macos-2026-10-11)
   are done.

## Ongoing tracks

- **Outgoing AI-analysis qualification.** Agree release targets and test EN/RU
  legal/native/OCR text, hidden source, identity joins/splits, ownership and
  correction time on a larger holdout ([open gates](docs/design/pseudonymization-production-readiness.md)).
  A continuous quality gate, not a release blocker.

## Engineering

- **Architecture refactoring** ([ADR 0049](docs/adr/0049-measured-stabilization-and-architecture-review.md)).
  Gated on must-fix item 3's profiling. Deliver case coordination and document
  lifecycle as separate, incremental projects: explicit owners for case
  persistence and synchronization, and explicit document transitions across the
  shell/view. GPUI keeps presentation, prompts and scheduling; editor/history,
  import and preview owners keep their contracts. Define each boundary before
  implementation. Retained editor layout is investigated only if release-build
  costs justify it; it is not committed work.
- **Build-cache check** ([ADR 0050](docs/adr/0050-rust-build-cache-growth.md),
  [measurements](docs/evidence/build-cache/2026-10-16/README.md)). Re-measure
  with `just disk` around 2026-10-23. Evaluate `sccache` only if `target/`
  exceeds about 30 GiB, accounting for its own disk use. Keep release settings;
  no automatic eviction or cache caps.
- **Cleanup charter** ([ADR 0029](docs/adr/0029-codebase-cleanup-charter.md)).
  Concrete ownership/test improvements. Conversion and OCR algorithm changes
  belong in the AnyDoc/pdf-inspector forks, backed by source/output comparisons.

## Deferred

- An optional case passphrase on top of the OS keyring
  ([ADR 0045](docs/adr/0045-prompt-free-case-key.md)).
- Exporting or sharing a case key (encrypted file plus passphrase), a
  cross-case dictionary, deliberate forgetting of originals, and stronger
  anonymization claims.
- Format-preserving pseudonymized DOCX/PDF export; an audit journal and
  compliance report of replacements; a re-identification risk hint.
- Bulk conversion beyond current lazy multi-file opening; design separately.
- Online formatting/OCR repair; provider, credentials, trust and review need decisions.
- Extra recognizers/full PII engines and optional source/page references; qualify
  a concrete workflow before expanding scope.

## Not planned

- Realistic fake-name pseudonyms: harder to restore and may collide with real
  names ([ADR 0018](docs/adr/0018-document-local-identity-review.md)).
- An irreversible redaction mode ([ADR 0022](docs/adr/0022-remove-anonymization-mode.md)).
- Watermarking and watched folders (outside AI preparation).
