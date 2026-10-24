# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap lists open work as of 2026-10-24 (latest: 0.1.5); accepted
behavior lives in [ADRs](docs/adr/), with terminology in [CONTEXT.md](CONTEXT.md).
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
0.1.1 (2026-10-22) fixes Windows single-instance launch, cases on unsaved
conversions, Save As and Recents, formatting next to spaces and lost focus
([CHANGELOG](CHANGELOG.md), [smoke run](docs/evidence/platform/2026-10-22/README.md)).
0.1.2 (2026-10-22) adds Find and Replace, keyboard reach and dialog semantics
for screen readers; its native checks are in the 0.2.0 acceptance run.
0.1.3 (2026-10-23) keeps visible mentions in place while stepping through
replacements (#102), keeps Quit available during model setup (#103), adds
recognition-language choice on the OCR card and fixes a crypto dependency
advisory. Its build was never published; 0.1.4 (same day) ships the same
changes.
0.1.5 (2026-10-23) makes Tab move between regions, makes Home's Recents and
Cases rows keyboard-operable, keeps the Keychain explainer's Continue
reachable, and adds expand/collapse all identities, Go to heading and WCAG
contrast checks.

## 0.1.x patches

Wrong output, data loss or silent failures that need no design decision
([milestone](https://github.com/lawkitt/mdoc/milestone/2)). Items that are not
ready ride along with a later patch.
Each patch gets a smoke run on both platforms; an item is done when its issue
closes, with the expected behavior stated there.

## 0.2.0

Waits only on must-fix items; should-fix items ship if ready, otherwise move
to the top of the next milestone. Items are in priority order.

### Must-fix

1. **[Platform] 0.2.0 acceptance.** An RC run of the full
   [checklist](docs/platform-acceptance.md) on both platforms, including S1,
   A5, D3 and D4 deferred from 0.1.0.
   Keyboard reach follow-ups found during implementation ([#89](https://github.com/lawkitt/mdoc/pull/89))
   also need native checks: F6 must stay in the Keychain explainer; Shift+F10
   must open a sidebar tab's case actions in both sidebar layouts; Categories
   to detect must take focus, hold Tab and return focus on Esc or Close.
   Previously F6 escaped the explainer and case categories required a pointer.
   Also natively: VoiceOver and Narrator read each Home Recents row's name,
   and Tab and ⇧Tab cycle the Keychain explainer's two buttons.
   *Done when:* the evidence folder covers every row; each Fail links an item.

### Should-fix

None open. The five from the rc.1 run (Windows headless renders, installed
Windows package smoke, model source and license, conversion clean-up, caret
affinity) shipped in 0.2.0-rc.2.

## Later

One line each, in rough order; promoted and written out at grooming. Earlier
detail: [ROADMAP.md before ADR 0059](https://github.com/lawkitt/mdoc/blob/c400853/ROADMAP.md#after-010).

- **[Workspace]** Folder drop flow: show what will open, ask for the key only
  for a case, cap large folders.
- **[OCR]** OS engines (Apple Vision, `Windows.Media.Ocr`) as the no-download
  default where they beat PP-OCR on our fixtures by character error rate; Vision
  reads Russian from macOS 13, Windows needs the Russian pack. *From Panoply.*
- **[Conversion]** Join hyphenated line breaks and normalize lists and blank
  lines at conversion, in the AnyDoc/pdf-inspector forks. *From MiaoYan's auto-format.*
- **[Perf]** Stress tier (2 MiB / 20,000 mentions) and numerical budgets.
- **[Models]** Show source and license, besides size, in the download prompt.
  *From Panoply.*
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
- **[Editor]** Caret affinity at soft wraps, so ⌘→ can sit after a row's last
  character when the row broke inside a word; today it stops one character
  short (ADR 0061). Table cells with wrapped text or
  `<br>` rows would then move by their own visual rows too.
- **[Tests]** CI smoke of the installed Windows package on a desktop runner,
  with screenshots. *From Panoply.*
- **[L10n]** Localized UI, Russian first; mdoc is to become multilingual.

## Ongoing tracks

- **Outgoing AI-analysis qualification.** A continuous quality gate, not a
  release blocker, for EN/RU legal text from PDF, DOCX and Markdown
  ([ADR 0016](docs/adr/0016-pii-production-qualification-scope.md)).
  *Done when:* numerical release targets are agreed from measured baselines on
  a larger holdout, with the measures and coverage ADR 0016 lists.

## Engineering

- **Architecture refactoring** ([ADR 0049](docs/adr/0049-measured-stabilization-and-architecture-review.md)):
  case coordination and document lifecycle as separate, incremental projects,
  each boundary defined before implementation.
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
