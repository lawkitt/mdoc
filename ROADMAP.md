# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap lists open work as of 2026-10-30 (latest: 0.2.1); accepted
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
left rows for the maintainer and found [#194](https://github.com/lawkitt/mdoc/issues/194).

## 0.2.x patches

None planned: fixes ride in 0.3.0-rc.1, and new bugs go to the
[0.3.0 milestone](https://github.com/lawkitt/mdoc/milestone/4). A severe bug
in 0.2.1 can still get a patch.

## 0.3.0

Restoring cases without surprises, then shortcut discovery
([milestone](https://github.com/lawkitt/mdoc/milestone/4)). The 2026-10-22 UX
review's polish shipped in 0.2.0. rc.1 is out and gets an install and smoke
check only; rc.2 follows the shelf and gets the full acceptance
([ADR 0083](docs/adr/0083-fewer-release-candidates.md), amended). Each item
merges to `main` on its own when green.

1. **[Shelf] Menu bar shelf** ([ADR 0084](docs/adr/0084-menu-bar-shelf.md)),
   macOS: a menu bar item listing pseudonymized documents to open,
   drag out or Copy for AI, with Pseudonymize Clipboard and Restore
   Clipboard. *Done when:* platform acceptance section 12 passes on rc.2,
   Windows is unchanged (no tray), and a test drags and copies ready,
   not-ready and changed-since-review documents and finds no original in any
   export.
2. **[Shelf] On by default, and polish** ([ADR 0084](docs/adr/0084-menu-bar-shelf.md),
   [polish grill](docs/design/menu-bar-grill-2026-10-31.md#polish-grill-after-263265-merged)):
   the switch on by default with the first-close note; prose previews with
   Markdown hidden, two lines and "…"; case labels; an outlined Copy for AI
   with "Copied" and ⌘1–⌘8; "N replaced" with categories on hover; the
   Clipboard section; one footer row; vibrancy; the drag cursor, card and
   hint. Lands before rc.2. *Done when:* rows M1–M3, M5, M7, M8 and M11 hold
   on a local build, headless renders show the polished shelf in both
   themes, and Windows is unchanged.

Records: [2026-10-29 grill](docs/design/roadmap-grill-2026-10-29.md),
[2026-10-30 grill](docs/design/roadmap-grill-2026-10-30.md), [shelf grill](docs/design/menu-bar-grill-2026-10-31.md). One RC is the
default ([ADR 0083](docs/adr/0083-fewer-release-candidates.md)): a must-fix
fix after rc.1 to the installer, signing, updates or the keychain forces
another, so such work lands first.

## iOS 0.1

The review-first iPhone and iPad app on the shared Rust core
([ADR 0085](docs/adr/0085-mobile-apps.md)). Core-crate changes merge when
green and leave desktop unchanged. Record:
[mobile grill](docs/design/mobile-grill-2026-10-31.md).

1. **[Mobile] Device measurements.** *From the feasibility spike
   ([report](docs/evidence/mobile/2026-10-31/README.md)).* The core, harness
   and Mac stand-in numbers are merged; run `scripts/ios-spike-device.sh` on
   the iPhone 17 Pro and iPhone 12 Pro, then decide the minimum iOS: the only
   approved iOS PDFium build needs iOS 26 (raise the minimum, or build PDFium
   for iOS 17). *Done when:* both device rows have pass or fail and the
   minimum iOS is settled in ADR 0085.
2. **[Mobile] TestFlight and acceptance.** *From the iOS build-out
   (#269, #272, #275–#278).* The app runs on the real core in the Simulator
   with parity, unit and UI tests in local CI; it has never been signed or
   run on a phone. Sign in to Xcode with the team's Apple ID, upload a
   TestFlight build, and run the iOS section of
   [platform acceptance](docs/platform-acceptance.md) (Device rows by hand).
   *Done when:* that section passes on a TestFlight build.

## Later

One line each, in rough order; promoted and written out at grooming. Earlier
detail: [ROADMAP.md before ADR 0059](https://github.com/lawkitt/mdoc/blob/c400853/ROADMAP.md#after-010).

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
- **[OCR]** Recover page structure on clean scans, in the AnyDoc/pdf-inspector forks.
- **[Cases]** System-wide Pseudonymize Clipboard hotkey, via the `global-hotkey`
  crate as in Panoply, with global shortcuts for the shelf and Restore
  Clipboard ([ADR 0084](docs/adr/0084-menu-bar-shelf.md)).
- **[Shelf]** Windows notification-area flyout under ADR 0084's rules, and
  Open at login (`SMAppService` on macOS) under the menu bar setting.
- **[L10n]** Localized UI, Russian first; mdoc is to become multilingual.
- **[Import]** Images (screenshots, photos of pages) through OCR, like Anoni's
  Image → text.
- **[Models]** Model choices show what each detects (Person, Email, Address…)
  and the memory of the active set ("2.3 GB in memory"). *From Anoni 0.86.1.*
- **[A11y]** Report keyboard focus to UI Automation in the Replacements list,
  the palette and the sidebar context menu: arrowing there leaves the window
  as the focused element, so Narrator can't follow. *From the 2026-10-26
  Windows run.*
- **[A11y]** Expose status text (download progress, "Download cancelled",
  "Checking runtime…", notices, a model's "Installed") to assistive tech as
  text and announcements. *From the 2026-10-26 Windows run (A5, S5).*
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
- **[Release]** Measure a cheaper release profile (thin LTO or more codegen
  units than `lto = true`, `codegen-units = 1`) against binary size and the
  perf budgets; the package step is 17–24 minutes. RCs and stables keep one
  profile. *From the 2026-10-30 grill.*
- **[Windows]** An all-users `.msi` again when an admin or buyer asks; it and
  the `.exe` detect each other and stop with a message; reuse UpgradeCode
  `3D50EAE8-D339-41B6-82BC-126389601487` ([ADR 0083](docs/adr/0083-fewer-release-candidates.md)).
- **[Mobile]** Resumed OCR joins pages with a blank line, so a list that
  continues across a page break differs from desktop; exact joins need
  pdf-inspector's page join made public in the fork (#276).

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
- **Release runs reuse `main`'s CI** ([ADR 0083](docs/adr/0083-fewer-release-candidates.md)):
  the `version` job skips the called CI when the release commit only bumps
  versions and the CHANGELOG and `main` passed its parent (waiting up to ~20
  minutes for a running one); `smoke-package` adds the Russian fixture.
  Land before 0.3.0-rc.1. *Done when:* a tag run skips CI on a green parent,
  runs it otherwise, and both platforms self-test both fixtures.

- **GUI checks without a takeover** (fleet ADR 0019): a debug-only switch
  (`MDOC_KEEP_RENDERING=1`) in the vendored `gpui-pre-macos` keeps the display
  link running while the window is covered, so the hub's background captures
  stay live. *Done when:* an experiment shows covered captures update and
  whether background keys reach GPUI, and `docs/development.md` (Fleet) says
  which GUI rows still need focus. *From the 0.3.0 away hub.*
- **Dev-build profile override:** debug builds read `MDOC_PROFILE_DIR`, so GUI
  checks run on a scratch profile instead of moving the user's
  `session.json` and `recents.json` aside. *From the 0.3.0 away hub.*
- **Local CI keeps the app binary per tested head** (`/ci/pr-N/<sha>/`), so
  the hub copies it for GUI checks instead of building a 30 GB worktree.
  *From the 0.3.0 away hub.*
- **Windows checkouts on a Dev Drive** (fleet ADR 0019): root checkout, lanes
  1–4 (lane 4 new) and local CI's home on `E:`; the move session follows
  `fleet/operations/2026-10-30-windows-dev-drive-move.md`.

## Deferred

- An optional case passphrase on top of the OS keyring
  ([ADR 0045](docs/adr/0045-prompt-free-case-key.md)).
- Exporting or sharing a case key (encrypted file plus passphrase), a
  cross-case dictionary, deliberate forgetting of originals, and stronger
  anonymization claims.
- Format-preserving pseudonymized DOCX and PDF export, and restoring an AI
  reply into DOCX. DOCX export with rdocx (and rdocx for the preview) was
  grilled and deferred on 2026-10-31
  ([record](docs/design/docx-export-grill-2026-10-31.md): Anoni's export, an
  rdocx prototype, 17 open questions). An audit journal and compliance
  report of replacements; a re-identification risk hint. For PDF, Panoply's `pdf.redact`
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
