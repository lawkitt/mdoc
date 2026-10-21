# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap lists open work as of 2026-10-20 (latest: 0.1.0); accepted
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

## 0.1.x patches

Wrong output, data loss or silent failures that need no design decision
([milestone](https://github.com/lawkitt/mdoc/milestone/1)). 0.1.1 ships when
item 1 is checked and items 2 and 3 are fixed; the rest ride along if ready.
Each patch gets a smoke run on both platforms; an item is done when its issue
closes, with the expected behavior stated there.

1. **[Platform] Windows caption buttons on the installed 0.1.0** ([#74](https://github.com/lawkitt/mdoc/issues/74)).
   *Done when:* the maintainer's T6 row passes on 0.1.0, or a fix ships.
2. **[Cases] Save as case on an unsaved conversion** creates an unopenable
   case with 0 documents ([#75](https://github.com/lawkitt/mdoc/issues/75)).
3. **[Windows] Single-instance launch:** a second launch starts another process
   that also writes the session and Recents ([#80](https://github.com/lawkitt/mdoc/issues/80)).
4. **[Workspace] Save As skips Recents** and creates the file with mode 0600
   ([#76](https://github.com/lawkitt/mdoc/issues/76)).
5. **[Editor] Closing marker after a trailing space** (`**delta **`)
   ([#77](https://github.com/lawkitt/mdoc/issues/77)).
6. **[Pseudonymization] Popup stays open after ⌘↩ Apply all**
   ([#78](https://github.com/lawkitt/mdoc/issues/78)).
7. **[UI] Focus lost** after Share › Save as case and after an undo that
   closes the popup ([#79](https://github.com/lawkitt/mdoc/issues/79)).
8. **[macOS] Quit leaves the process running** with no windows
   ([#83](https://github.com/lawkitt/mdoc/issues/83)).

## 0.2.0

Waits only on must-fix items; should-fix items ship if ready, otherwise move
to the top of the next milestone. Items are in priority order.

### Must-fix

1. **[Editor] Enter starts a paragraph.** *From the rc.1 run.* Enter inserts one
   `\n`, so Copy for AI joins lines the editor shows apart. Decide Enter
   (`\n\n`) versus ⇧Enter (line break) and how lists, tables and headings keep
   their own Enter.
   *Done when:* the editor after Enter matches a CommonMark rendering of the
   copied Markdown, with tests on paragraphs, lists and tables.
2. **[Pseudonymization] Popup Enter follows the scope.** *From the rc.1 run.*
   The popup shows All N, but Enter applies only this mention and switches the
   scope. Decide which wins; amend ADR 0048/0055.
   *Done when:* Enter, the scope segments and Apply agree, tested per scope.
3. **[Editor] macOS caret moves.** *From the RC runs.* ⌘←/→/↑/↓ (with ⇧);
   Windows has its Ctrl moves.
   *Done when:* each move works on macOS, with tests.
4. **[A11y] Keyboard reach.** *From the RC runs (A4, H18).* Plain Tab cannot
   reach the toolbar or sidebar. Likely F6/⇧F6 between sidebar, toolbar and
   editor, and Tab from Home into its controls.
   *Done when:* every control is reachable by keyboard alone on both platforms.
5. **[Platform] 0.2.0 acceptance.** An RC run of the full
   [checklist](docs/platform-acceptance.md) on both platforms, including S1,
   A5, D3 and D4 deferred from 0.1.0.
   *Done when:* the evidence folder covers every row; each Fail links an item.

### Should-fix

1. **[Editor] Find and replace.** *From the 2026-10-17 grooming.* Replace,
   Replace all, Match case, Whole word; ported from Zed's GPL-3.0 `search`
   crate with credit. Replace all is one undo step and skips applied aliases.
   ⌘⌥F / Ctrl+H.
   *Done when:* each control works by keyboard and mouse on a long document
   with applied aliases, and one undo reverts Replace all.
2. **[A11y] Dialog and role semantics.** *From the RC runs (A3).* Settings is
   not a dialog; model choices read as checkboxes; narrow Markdown/Source read
   as buttons; close buttons say only "Close tab"; the unsaved-changes dialog is
   titled "Warning".
   *Done when:* VoiceOver and Narrator announce each with its proper role and name.
3. **[UI] Small polish.** *From the dry run and RC runs.* Toggle Sidebar
   shortcut (⌃⌘S); same-name tabs disambiguated by source type, not full path;
   the DOCX notice only when the file has such content; wrapped search matches
   painting the previous line; the DOCX badge after Save As; the window title on
   Home; Finder drop not activating mdoc; unsupported-file drop feedback and
   the Notices popover; the preview Find button's ⌘F hint; the theme tooltip;
   the setup card's size after Cancel.
   *Done when:* each point is fixed or moved to its own item.
4. **[OCR] Pick the recognition language.** *From the rc.1 run.* Default to
   English & Russian when the text layer, file name or a Russian input source
   suggests it.
   *Done when:* the Russian OCR fixture gets English & Russian by default and
   the English one stays English, with the choice explained on the card.

## Later

One line each, in rough order; promoted and written out at grooming. Earlier
detail: [ROADMAP.md before ADR 0059](https://github.com/lawkitt/mdoc/blob/c400853/ROADMAP.md#after-010).

- **[A11y]** Expose editor text to screen readers (AccessKit text role, ranges,
  caret and selection events); first to promote.
- **[Workspace]** Folder drop flow: show what will open, ask for the key only
  for a case, cap large folders.
- **[Perf]** Stress tier (2 MiB / 20,000 mentions) and numerical budgets.
- **[UI]** Center and smoothly scroll to the selected mention.
- **[UI]** Expand/collapse all in the Replacements panel.
- **[PII]** Merge hint for identities that very likely name the same subject.
- **[UI]** Settings redesign with a category list and one row per setting.
- **[Cases]** Case page (documents, identities, rename, recolor, delete); ADR first.
- **[UI]** Workspace Back/Forward.
- **[OCR]** Recover page structure on clean scans, in the AnyDoc/pdf-inspector forks.
- **[DOCX]** Pseudonymized DOCX export with rdocx, then rdocx for the preview.
- **[Cases]** System-wide Pseudonymize Clipboard hotkey.

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
  re-measure with `just disk` on 2026-10-23 (28 GiB on 2026-10-20); evaluate
  `sccache` only above about 30 GiB.
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
  replacements; a re-identification risk hint.
- Bulk conversion beyond current lazy multi-file opening; design separately.
- Online formatting/OCR repair; provider, credentials, trust and review need decisions.
- Extra recognizers/full PII engines and optional source/page references; qualify
  a concrete workflow before expanding scope.

## Not planned

- Realistic fake-name pseudonyms: harder to restore and may collide with real
  names ([ADR 0018](docs/adr/0018-document-local-identity-review.md)).
- An irreversible redaction mode ([ADR 0022](docs/adr/0022-remove-anonymization-mode.md)).
- Watermarking and watched folders (outside AI preparation).
- [xerj](https://github.com/xerj-org/xerj) (local search server for agents): no
  product fit for a single-document editor; a local server and its optional
  off-machine reranker conflict with offline operation.
- Replacing AnyDoc with rdocx for DOCX-to-Markdown conversion: conversion
  changes belong in the AnyDoc fork ([ADR 0029](docs/adr/0029-codebase-cleanup-charter.md)).
