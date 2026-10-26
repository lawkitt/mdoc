# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap lists open work as of 2026-10-26 (latest: 0.1.5; 0.2.0-rc.4
tagged); accepted behavior lives in [ADRs](docs/adr/), with terminology in [CONTEXT.md](CONTEXT.md).
How it is kept: [ADR 0059](docs/adr/0059-roadmap-after-0-1-0.md) and
[Roadmap upkeep](docs/development.md#roadmap-upkeep). Bugs go to
[Issues](https://github.com/lawkitt/mdoc/issues).

## Current status

0.1.0 shipped on 2026-10-19 for Apple Silicon macOS and Windows x64 after
native acceptance ([evidence](docs/evidence/platform/2026-10-19/README.md),
[ADR 0057](docs/adr/0057-native-platform-acceptance.md)): local conversion with
PDF/DOCX previews, offline OCR, assistive pseudonymization
([ADR 0034](docs/adr/0034-pseudonymization-supported-feature.md)), encrypted
cases with AI-reply restoration ([ADR 0041](docs/adr/0041-cases-and-ai-response-restoration.md))
and EN/RU spellcheck. No PII model has passed the
[initial qualification](tests/fixtures/pseudonymization/README.md).
0.1.1 to 0.1.5 (2026-10-22 and 2026-10-23) fixed launch, cases, editing and
quit bugs and added Find and Replace, keyboard reach for screen-reader users
and EN/RU recognition-language choice ([CHANGELOG](CHANGELOG.md),
[smoke run](docs/evidence/platform/2026-10-22/README.md)).
0.2.0-rc.1 passed the macOS acceptance run on 2026-10-24 with one fail (#115,
since fixed); Windows did not run
([evidence](docs/evidence/platform/2026-10-24/README.md)). rc.2 and rc.3 got
no Windows run. The architecture refactoring
([ADR 0069](docs/adr/0069-app-architecture-and-code-ownership.md)) finished on
2026-10-26, so 0.2.0 now ships from rc.4, cut from `main`, after full
acceptance on both platforms.

## 0.1.x patches

Wrong output, data loss or silent failures that need no design decision
(GitHub milestones). None open; after 0.2.0 ships, patches are 0.2.x. Items
that are not ready ride along with a later patch.
Each patch gets a smoke run on both platforms; an item is done when its issue
closes, with the expected behavior stated there.

## 0.2.0

Waits only on must-fix items; should-fix items ship if ready, otherwise move
to the top of the next milestone. Items are in priority order.

### Must-fix

1. **[Platform] 0.2.0 acceptance.** A run of the full
   [checklist](docs/platform-acceptance.md) on rc.4 on both platforms,
   including S1, A5, D3 and D4 deferred from 0.1.0 and, on Windows, the
   model-download rows S2–S5. rc.1's macOS run predates the refactoring, so
   macOS runs again.
   Keyboard reach follow-ups found during implementation ([#89](https://github.com/lawkitt/mdoc/pull/89))
   also need native checks: F6 must stay in the Keychain explainer; Shift+F10
   must open a sidebar tab's case actions in both sidebar layouts; Categories
   to detect must take focus, hold Tab and return focus on Esc or Close.
   Previously F6 escaped the explainer and case categories required a pointer.
   Also natively: VoiceOver and Narrator read each Home Recents row's name,
   and Tab and ⇧Tab cycle the Keychain explainer's two buttons.
   *Done when:* the evidence folder covers every row; each Fail links an item.

### Should-fix

None open.

## 0.3.0

Closing and restoring cases without surprises, then shortcut discovery
([milestone](https://github.com/lawkitt/mdoc/milestone/4)). The 2026-10-22 UX
review's polish shipped in 0.2.0. Items are in priority order.

1. **[Cases] One close prompt.** Closing a pseudonymized, unsaved tab asks
   twice (keep the mapping, then save changes), and ⌘Q drops an unsaved
   mapping without asking ([#119](https://github.com/lawkitt/mdoc/issues/119)).
   One prompt covers both, quit asks about mappings too and offers Discard
   all. Amends [ADR 0041](docs/adr/0041-cases-and-ai-response-restoration.md);
   ADR first. *From the UX review and the rc.1 run.*
   *Done when:* the ADR is accepted and #119 closes.
2. **[Cases] Restore banner wording.** Say plainly that restored names are
   proposals and what Apply does. *From the UX review.*
   *Done when:* the banner's first sentence says the names are proposals and
   what Apply does, in the headless renders.
3. **[UI] Keyboard shortcuts panel.** `?` and a palette action list every
   shortcut by context (everywhere, editor, popup, panel). *From Anoni 0.86.1.*
   *Done when:* every bound action appears, from one source of truth.
4. **[UI] Theme control.** A System / Light / Dark segmented control in
   Settings; today the sidebar button cycles without showing the choices.
   *From Anoni 0.86.1.* *Done when:* the choice is visible and persists.

## Later

One line each, in rough order; promoted and written out at grooming. Earlier
detail: [ROADMAP.md before ADR 0059](https://github.com/lawkitt/mdoc/blob/c400853/ROADMAP.md#after-010).

- **[Workspace]** Folder drop flow: show what will open, ask for the key only
  for a case, cap large folders.
- **[OCR]** OS engines (Apple Vision, `Windows.Media.Ocr`) as the no-download
  default where they beat PP-OCR on our fixtures by character error rate; Vision
  reads Russian from macOS 13, Windows needs the Russian pack. *From Panoply.*
- **[Perf]** Stress tier (2 MiB / 20,000 mentions) and numerical budgets.
- **[A11y]** Expose editor text to screen readers (AccessKit text role, ranges,
  caret and selection events); promote when a user asks or a buyer needs it
  for compliance.
- **[UI]** Smooth scrolling to the selected mention, and maybe centering it;
  whether to center is still open, and either way it must not undo #102's
  keep-visible-in-place behavior.
- **[PII]** Merge hint for identities that very likely name the same subject.
- **[UI]** Settings redesign with a category list and one row per setting.
- **[Cases]** Case page (documents, identities, rename, recolor, delete); ADR first.
- **[UI]** Workspace Back/Forward.
- **[OCR]** Recover page structure on clean scans, in the AnyDoc/pdf-inspector forks.
- **[DOCX]** Pseudonymized DOCX export with rdocx, then rdocx for the preview.
- **[Cases]** System-wide Pseudonymize Clipboard hotkey, via the `global-hotkey`
  crate as in Panoply.
- **[L10n]** Localized UI, Russian first; mdoc is to become multilingual.
- **[Import]** Images (screenshots, photos of pages) through OCR, like Anoni's
  Image → text.
- **[PII]** Selection bar: selecting text shows a small "Pseudonymize as…"
  bar, a pointer path to ⌘⌥P with the category picked in place. *From Anoni 0.86.1.*
- **[Cases]** Restore in place: a reply box in Share that restores live, with
  a Reply view and "N values restored · M unrecognised"; today a reply opens
  its own tab (ADR 0041). ADR first. *From Anoni 0.86.1.*
- **[Workspace]** Per-document history of decisions ("What you did": applied,
  kept, renamed, with time), next to undo. ADR first. *From Anoni 0.86.1.*
- **[Models]** Model choices show what each detects (Person, Email, Address…)
  and the memory of the active set ("2.3 GB in memory"). *From Anoni 0.86.1.*
- **[PII]** Default categories to detect for new documents in Settings,
  besides per-case categories. *From Anoni 0.86.1.*

## Ongoing tracks

- **Outgoing AI-analysis qualification.** A continuous quality gate, not a
  release blocker, for EN/RU legal text from PDF, DOCX and Markdown
  ([ADR 0016](docs/adr/0016-pii-production-qualification-scope.md)).
  *Done when:* numerical release targets are agreed from measured baselines on
  a larger holdout, with the measures and coverage ADR 0016 lists.

## Engineering

- **Build-cache check** ([ADR 0050](docs/adr/0050-rust-build-cache-growth.md)):
  `just disk` read 38.5 GiB on 2026-10-23, over the ~30 GiB threshold; evaluate
  `sccache` against a periodic `cargo clean --workspace`.
- **Cleanup charter** ([ADR 0029](docs/adr/0029-codebase-cleanup-charter.md)):
  concrete ownership/test improvements; conversion and OCR changes belong in
  the AnyDoc/pdf-inspector forks.

## Deferred

- An optional case passphrase on top of the OS keyring
  ([ADR 0045](docs/adr/0045-prompt-free-case-key.md)).
- Exporting or sharing a case key (encrypted file plus passphrase), a
  cross-case dictionary, deliberate forgetting of originals, and stronger
  anonymization claims.
- Format-preserving pseudonymized PDF export and restoring an AI reply into
  DOCX (DOCX export is in Later); an audit journal and compliance report of
  replacements; a re-identification risk hint. For PDF, Panoply's `pdf.redact`
  shape fits: redraw only matched pages with aliases painted in, an invisible
  text layer, matched metadata dropped, and a leak check before saving.
- Agent access (CLI or MCP, as in Panoply and MiaoYan) only where the agent
  receives text the user reviewed in mdoc, then restores through the case;
  design and trust model separately.
- Bulk conversion beyond current lazy multi-file opening; design separately.
- Online formatting/OCR repair; provider, credentials, trust and review need decisions.
- Extra recognizers/full PII engines and optional source/page references; qualify
  a concrete workflow before expanding scope.

## Not planned

- Realistic fake-name pseudonyms: harder to restore and may collide with real
  names ([ADR 0018](docs/adr/0018-document-local-identity-review.md)).
- An irreversible redaction mode ([ADR 0022](docs/adr/0022-remove-anonymization-mode.md)).
- Watermarking and watched folders (outside AI preparation).
- MiaoYan's notes-app features (slides, wikilinks, Mermaid and LaTeX, slash
  templates, a synced notes library): outside AI preparation, and
  [ADR 0030](docs/adr/0030-render-standard-markdown-only.md) renders standard Markdown only.
- Document version history: snapshots would keep client names in plaintext
  outside the case and the file lifecycle ([ADR 0003](docs/adr/0003-file-based-document-lifecycle.md)).
- An editor-wide Markdown reformat: it moves every proposal and alias; cleanup
  belongs at conversion (Later).
- [xerj](https://github.com/xerj-org/xerj) (local search server for agents): no
  product fit for a single-document editor; a local server and its optional
  off-machine reranker conflict with offline operation.
- Replacing AnyDoc with rdocx for DOCX-to-Markdown conversion: conversion
  changes belong in the AnyDoc fork ([ADR 0029](docs/adr/0029-codebase-cleanup-charter.md)).
