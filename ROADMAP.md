# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap lists open work as of 2026-10-27 (latest: 0.2.1); accepted
behavior lives in [ADRs](docs/adr/), with terminology in [CONTEXT.md](CONTEXT.md).
How it is kept: [ADR 0059](docs/adr/0059-roadmap-after-0-1-0.md) and
[Roadmap upkeep](docs/development.md#roadmap-upkeep). Bugs go to
[Issues](https://github.com/lawkitt/mdoc/issues).

## Current status

0.2.0 shipped on 2026-10-26 for Apple Silicon macOS and Windows x64, from
`main` after the architecture refactoring
([ADR 0069](docs/adr/0069-app-architecture-and-code-ownership.md)) and the
rc.4 acceptance run on both platforms
([evidence](docs/evidence/platform/2026-10-26/README.md)). Its must-fix fail
(#167) was fixed and re-checked on macOS only; the maintainer skipped the
remaining manual rows and the keychain launch check. Windows also ships an
`.msi`. No PII model has passed the
[initial qualification](tests/fixtures/pseudonymization/README.md). Earlier
releases: [CHANGELOG](CHANGELOG.md). 0.2.1 (2026-10-27) fixed the seven
bugs from the 0.2.0 runs; its [smoke run](docs/evidence/platform/2026-10-27/README.md)
left rows for the maintainer and found #194 (now 0.3.0 item 5).

## 0.2.x patches

None planned: fixes ride in 0.3.0-rc.1, and new bugs go to the
[0.3.0 milestone](https://github.com/lawkitt/mdoc/milestone/4). A severe bug
in 0.2.1 can still get a patch.

## 0.3.0

Restoring cases without surprises, then shortcut discovery
([milestone](https://github.com/lawkitt/mdoc/milestone/4)). The 2026-10-22 UX
review's polish shipped in 0.2.0. Items are in priority order. Until
0.3.0-rc.1 is cut, an item added here with a Done-when line is in scope (no
five-item box this cycle); rc.1 is cut once every item is done. Each item
merges to `main` on its own when green.

3. **[UI] Keyboard shortcuts panel.** `?` and a palette action list every
   shortcut by context (everywhere, editor, popup, panel). *From Anoni 0.86.1.*
   *Done when:* every bound action appears, from one source of truth.
4. **[UI] Theme control.** A System / Light / Dark segmented control in a
   new Settings → Appearance section at the top; today the sidebar button cycles without showing the choices.
   *From Anoni 0.86.1.* *Done when:* the choice is visible and persists.
5. **[A11y] One visible focus style.** Sidebar rows and Settings show focus
   with the same fill as selection or hover, and a toolbar toggle's "on" fill
   hides its ring ([#194](https://github.com/lawkitt/mdoc/issues/194)), on both
   platforms. One focus ring for every control, distinct from selection and
   hover. *From the rc.4 and 0.2.1 runs.* *Done when:* every Tab stop shows the
   same ring in light and dark, including toggles that are on, and #194 closes.

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
- **[A11y]** Report keyboard focus to UI Automation in the Replacements list,
  the palette and the sidebar context menu: arrowing there leaves the window
  as the focused element, so Narrator can't follow. *From the 2026-10-26
  Windows run.*
- **[A11y]** Expose status text (download progress, "Download cancelled",
  "Checking runtime…", notices, a model's "Installed") to assistive tech as
  text and announcements. *From the 2026-10-26 Windows run (A5, S5).*
- **[Cases]** Save mapping as case on an unsaved conversion says why Save As
  opens ("save the document first so the case can find it"). *From the
  2026-10-26 Windows run (E4).*
- **[Models]** Settings remembers model install state instead of showing
  "Checking…" on every open, and says whether the OCR choice made on a
  document's card also becomes the default. *From the 2026-10-26 Windows
  run (S5).*
- **[Windows]** Caption buttons and resize borders respond to synthetic input
  (UI Automation, Voice Access, test agents); today only a physical mouse
  works, and a narrow window leaves almost no title bar to drag, so a
  double-click can land on Pseudonymize. *From the 2026-10-26 Windows run
  (T6, D5).*
- **[Release]** Keep the Windows acceptance helpers (UI Automation tree and
  focus dump, clipboard byte compare, HTML clipboard writer) in `scripts/` so
  runs don't rewrite them. *From the 2026-10-26 Windows run.*
- **[A11y]** Report keyboard focus on macOS too: only text fields and dialog
  buttons set `AXFocused`, so accessibility clients and agent-run acceptance
  can't follow focus elsewhere. *From the rc.4 acceptance run.*
- **[A11y]** F6 into the sidebar lands on the active tab, not the last control
  (often Settings, where ↑/↓ do nothing), so ↑/↓, ⌫ and the tab's actions are
  one key away. *From the rc.4 acceptance run.*
- **[A11y]** A Mac shortcut for a sidebar tab's actions besides ⇧F10, which on
  MacBook keyboards needs fn (F10 is the mute key). *From the rc.4 acceptance run.*
- **[Windows]** One install per machine: the `.msi` (all users) and the NSIS
  `.exe` (per user) each detect the other and stop with a message, and the
  updater says which one it updates. Today both can sit side by side, with two
  shortcuts and two `mdoc.exe`. *From the 0.2.1 smoke run.*

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
- **Update check before publishing:** `just smoke-update` from the last
  stable release can only pass once the new stable is published, because a
  stable build reads only `/releases/latest`; let the smoke run point the
  updater at a named draft or pre-release. *From the rc.4 acceptance run.*
- **Keychain launch check without a fresh account:** run
  `scripts/keychain-launch-check.sh` and E2 against a throwaway keychain. The
  macOS `acceptance` user (fleet ADR 0011) shows the first Keychain prompt
  only once, since the case-key item outlives a profile rename and agents
  never delete it. *From the rc.4 acceptance run and the 0.2.1 grill.*
- **Windows CI timing on slow runners:** ADR 0070 targets about 8 minutes
  for `test (windows-latest)` on a warm cache; #197 was measured only with
  partial caches on EPYC 9V45/9V74. *Done when:* a `main` run with a full
  cache match on an EPYC 7763 or Xeon 8370C runner is recorded in ADR 0070,
  and a miss gets its own grill.
- **Release-run rules:** a patch publishes once both agent runs are done with
  no must-fix Fail; rows only the maintainer can do may follow. Update
  `docs/release-run.md` and ADR 0059's acceptance line. (The GUI runner,
  a Claude desktop session in the `acceptance` account, is already in
  `docs/release-run.md`.) *From the 0.2.1 run.*

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
