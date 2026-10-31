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

1. **[Mobile] TestFlight and acceptance.** *From the iOS build-out
   (#269, #272, #275–#278).* The app runs on the real core in the Simulator
   with parity, unit and UI tests in local CI; it has never been signed or
   run on a phone. Sign in to Xcode with the team's Apple ID, upload a
   TestFlight build, and run the iOS section of
   [platform acceptance](docs/platform-acceptance.md) (Device rows by hand).
   The first build waits for the measured floor and the device checklist
   (P17). *Done when:* that section passes on a TestFlight build.
2. **[Mobile] UI polish.** *From the [iOS polish grill](docs/design/ios-polish-grill-2026-10-31.md)
   ([ADR 0087](docs/adr/0087-ios-ui-polish.md)).* Four PRs: defects and the
   survey test, Markdown escapes in `mdoc-markdown`, visual polish, flows.
   *Done when:* the after survey sits beside the before set and P12, P13
   and P16 pass in the Simulator.
3. **[Mobile] Device UI tests on Wi-Fi.** *From the device grill
   (#295).* `just ios-device-test <udid>` is merged and its review flow
   passed on the iPhone 17 Pro; the long-jobs test (a real text-recognition
   download, then 45 s away during Find names) stopped at its Wi-Fi check
   on 5G. *Done when:* the whole run passes on the iPhone 17 Pro on Wi-Fi.
4. **[Mobile] Device tests below the floor.** *From the device grill
   ([ADR 0088](docs/adr/0088-ios-on-real-devices.md)).* The long-jobs test
   needs the detector tier, so nothing checks a phone below the 8 GB floor:
   the text-recognition download, a scan and a .docx import that survive
   leaving the app, the below-floor note where Find names would be (and in
   Settings), and marking a name by selecting it. *Done when:* `just
   ios-device-test` runs a below-floor test on the iPhone 12 Pro and it
   passes, with no new crash report. The test is in #296; on the 12 Pro the
   runner timed out enabling automation, so turn on Settings → Developer →
   UI Automation there first.

## Android 0.1

The review-first Android app on the shared Rust core, built beside iOS 0.1
on the Windows PC ([ADR 0089](docs/adr/0089-android-app.md)). Core-crate
changes merge when green and leave desktop and iOS unchanged. Record:
[Android grill](docs/design/android-grill-2026-10-31.md), whose
Maintainer actions list the steps on the maintainer's side.

1. **[Android] Toolchain on Windows.** Pinned SDK, NDK r29, Temurin 21 and
   Gradle wrapper; `scripts/build-android-core.sh` with ONNX Runtime 1.27.0
   and PDFium 7988; `just android-core`. *Done when:* on a fresh checkout
   on `windows`, `just android-core` writes both ABIs and the Kotlin
   bindings to `target/android/`, every download verified and every `.so`
   16 KB aligned.
2. **[Android] Feasibility spike.** `tools/android-spike` and
   `scripts/android-spike-device.sh` on Test Lab phones at 6, 8 and 12 GB
   or more. *Done when:* each row has pass or fail
   against ADR 0085's budgets and Android's floor constant is set from them.
3. **[Android] App scaffold and the Android job.** `apps/android` over
   `mdoc-ffi` with pinned, verified Gradle dependencies; JVM parity tests;
   the Android job in `local/windows`. *Done when:* a PR touching
   `apps/android` gets a green `local/windows` with the Android job, and the
   parity tests match the desktop goldens, Copy for AI bytes included.
4. **[Android] Screens in iOS order.** Home, intake (Paste, Files, Photos,
   Scan), setup and downloads, review and Identities, the hand-off bar,
   Restore with mdoc, Settings. *Done when:* the emulator test walks open,
   review, share, restore, and TalkBack reads and acts on candidates.
5. **[Android] Long jobs on real phones.** The foreground service, the
   Wi-Fi rule and resume. *Done when:* on the emulator and on a Test Lab
   Pixel and Samsung phone an import, OCR and Find names each survive
   leaving the app and a process kill, with no crash or ANR in
   `ApplicationExitInfo`; and a model download does too on the emulator
   and the friend's phone.
6. **[Android] UI survey.** `just android-survey`. *Done when:* the before
   and after sets sit under `docs/evidence/android-app/`.
7. **[Android] Device UI tests.** `just android-device-test`. *Done when:*
   it passes on one Test Lab phone per RAM class.
8. **[Android] Play closed test.** The Play account, the device check and
   12 testers. *Done when:* the closed test has had 12 testers opted in for
   14 days in a row.
9. **[Android] Acceptance and `android-v0.1.0`.** Platform acceptance
   section 14 (A1–A17). *Done when:* the section passes on the release
   build, the signed APK and its SHA-256 are on GitHub Releases, and the
   Play production release is submitted (or the slip is recorded).

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
- **[Mobile]** A Markdown table row wider than an iPhone wraps mid-row in
  the review text (the 2026-10-31 survey's settlement table), so columns no
  longer line up. The source is shown as is (ADR 0085), so a fix needs a
  table block that scrolls sideways without re-rendering or moving offsets.
- **[Detection]** The English settlement demo doesn't scan clean: the claim
  number "6001842/2026" is not proposed (the captured-reply fixture treats it
  as an identifier), and the role word "Employer" is replaced as an
  organisation (ORG_2), which garbles the terms. The demos are the first thing
  a new user scans; worth a grill on identifier coverage and role words.
  *From the 2026-10-31 shelf GUI run.*
- **[Spelling]** Russian name forms such as "Маришей" get a misspelling
  underline; names, detected mentions and aliases shouldn't be spell-checked.
  *From the 2026-10-31 shelf GUI run.*
- **[A11y]** The shelf row is a button with its Copy for AI button nested
  inside, so accessibility hit-testing lands on the row (an agent's click on
  Copy opened the document instead); check with VoiceOver (M9) and make them
  siblings if it can't reach the button. *From the 2026-10-31 shelf GUI run.*
- **[Release]** Keep the macOS acceptance helpers (window list with layers,
  AX dump and press by title, pressing the menu bar item) in `scripts/`
  beside the Windows ones, so GUI runs don't rewrite them. *From the
  2026-10-31 shelf GUI run.*
- **[Detection]** A name written with an HTML entity (`Ann&#97; Petrova`)
  is neither detected nor caught by the readiness check, which looks for
  known originals in the raw text; Copy for AI then hands it to the AI, which
  reads it as the name. The shelf preview and drag card mask it since #280;
  detection and Copy for AI need the same decoding. *From the 2026-10-31
  shelf polish review.*
- **[Shelf]** Review the shelf preview's Markdown block reading (`src/shelf.rs`,
  `preview_lines`) against CommonMark as a whole rather than case by case:
  round after round of Codex review on #280 found another container or
  fence edge.
  Still open: leaving a nested list item drops the outer one too, so a fence
  back in the outer item (`- Notes` / `  - More` / blank / `  ~~~`) hides a
  later sibling's prose. The failure modes are a missing preview or code
  shown as prose; originals stay masked (a preview that still spells a known
  name is refused). Consider reusing a real block parser. *From the
  2026-10-31 shelf polish review.*

The 2026-10-31 UI/UX survey (build `84fafd3`, macOS, Light and Dark) adds
the items below, polish first, then ideas. Its twelve bugs are issues
[#300](https://github.com/lawkitt/mdoc/issues/300)–[#311](https://github.com/lawkitt/mdoc/issues/311)
in the [0.3.0 milestone](https://github.com/lawkitt/mdoc/milestone/4). Repro
steps and screenshots: [evidence](docs/evidence/ui-ux-survey/2026-10-31/README.md)
(IDs U1–U31).

- **[Editor] Polish: alias chips touch the next character.** The apostrophe
  of "PERSON_2's" sits on the chip border; on italic text the ring covers
  half of it ("ORG_4's"); the popup's alias field draws its pencil flush
  against the alias. Add a hairline of outside spacing, slanted for italics.
  ([U13](docs/evidence/ui-ux-survey/2026-10-31/README.md#u13-alias-chips-touch-or-cover-the-next-character))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Polish: one action, several names.** Restore AI reply / Paste AI
  Response / Paste AI reply…; Save as case / Save mapping as case; Before /
  After / Side by side; UK and US spelling mixed ("unrecognised", "licence",
  "organisations" beside "Organization", "License"). Pick one name and one
  spelling ([CONTEXT.md](CONTEXT.md)) and use it in menus, palette and
  buttons. ([U14](docs/evidence/ui-ux-survey/2026-10-31/README.md#u14-one-action-several-names))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Polish: the menus miss common items.** No File › Open Recent, no
  Restore Clipboard beside Pseudonymize Clipboard, no View items for Home,
  Search, Original / Pseudonymized / Side by side or the Replacements panel,
  no mdoc Help in Help, no Check for Updates…; Copy for AI and Copy Markdown
  have no shortcut. Fits the 0.3.0 shortcut-discovery theme.
  ([U15](docs/evidence/ui-ux-survey/2026-10-31/README.md#u15-menus))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Polish: tooltips leave out shortcuts.** Sidebar tooltips (Collapse
  sidebar, Back, Forward, New tab) show none though ⌃⌘S, ⌘[ and ⌘] exist;
  the toolbar's Pseudonymize (⇧⌘P) and the popup's alias field have no
  tooltip; "Documents stay on this Mac" opens its tooltip over the button
  beside it; "Switch theme (Light)" in the palette reads as current or next.
  ([U16](docs/evidence/ui-ux-survey/2026-10-31/README.md#u16-tooltips))
  *From the 2026-10-31 UI/UX survey.*
- **[Settings] Polish: states that look alike.** An enabled "Remove…" looks
  like the active model's disabled one; toggles don't share a column and off
  toggles stay accent-tinted; the selected and the focused Appearance segment
  look the same; About ends with a disabled Reset that has nothing to reset;
  long pages cut a row with no scroll cue.
  ([U17](docs/evidence/ui-ux-survey/2026-10-31/README.md#u17-settings))
  *From the 2026-10-31 UI/UX survey.*
- **[Import] Polish: error and OCR cards offer the wrong next step.** Damaged
  and password-protected files offer only "Try again" while the Source pane
  takes 60% to say there is no preview; the all-scanned card offers "Use
  native text only" when no page has text, and draws it above "Run OCR";
  OCR of blank pages returns stray "I"s with no "no text found"; "!" means
  both "needs OCR" and "failed". Offer Close and Open another…, collapse the
  empty pane, and reword.
  ([U18](docs/evidence/ui-ux-survey/2026-10-31/README.md#u18-import-error-and-ocr-cards))
  *From the 2026-10-31 UI/UX survey.*
- **[Cases] Polish: Save as case asks for the file first.** On an unsaved
  conversion it shows the Save panel, then the Keychain explainer; Not now
  leaves the file saved, no case and no message. Show the explainer first, or
  say what happened. ([U19](docs/evidence/ui-ux-survey/2026-10-31/README.md#u19-save-as-case-asks-for-a-file-first-then-for-the-keychain))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Polish: native panels ignore the app theme.** With Appearance on
  Light under a Dark macOS, the Save panel, its alerts and the menus stay
  dark; set the app's `NSAppearance` from the setting.
  ([U20](docs/evidence/ui-ux-survey/2026-10-31/README.md#u20-native-panels-ignore-the-app-theme))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Polish: narrow windows.** At 720×480 with the sidebar and
  Replacements open the editor is about 270 pt (three or four words a line);
  the sidebar is a fixed 180 pt, so names truncate early and shared prefixes
  look alike. Collapse the sidebar below a width, let it be dragged, and
  truncate names in the middle.
  ([U21](docs/evidence/ui-ux-survey/2026-10-31/README.md#u21-narrow-windows))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Polish: dialogs.** Save changes rings Save but doesn't fill it,
  orders Save · Discard · Cancel unlike macOS, and has no scrim in Dark; the
  Quit summary says "3 documents have unsaved changes or mappings" without
  naming them. ([U22](docs/evidence/ui-ux-survey/2026-10-31/README.md#u22-dialogs))
  *From the 2026-10-31 UI/UX survey.*
- **[Shelf] Polish: small shelf details.** A converted document's preview
  ends "1...."; the gear's tooltip is clipped at the shelf edge; the "Names
  from … ▾" chooser replaces the document list at the top, far from its
  trigger, and the shelf jumps in height. Beyond the 0.3.0 shelf polish item.
  ([U23](docs/evidence/ui-ux-survey/2026-10-31/README.md#u23-shelf-details))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Polish: the floating view bar.** It shows only near the bottom
  edge or while scrolling, its zoom pill covers the line beneath, and a
  hovered segment looks selected.
  ([U24](docs/evidence/ui-ux-survey/2026-10-31/README.md#u24-the-floating-view-bar))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Polish: small alignment and style items.** "Open a case…" ends 7 pt
  short of the line above it; "Next unrecognised" is borderless beside
  bordered buttons; the Reply view's chip and Close don't align; Home's
  first Tab stop is the toolbar bell.
  ([U25](docs/evidence/ui-ux-survey/2026-10-31/README.md#u25-small-alignment-and-style-items))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Polish: Pseudonymize clipboard on Home looks broken.** It jumps to
  the last document tab and replaces only that document's names, so new text
  comes back "unchanged". Say on Home which names it uses, or let it pick a
  source as the shelf does.
  ([U26](docs/evidence/ui-ux-survey/2026-10-31/README.md#u26-pseudonymize-clipboard-from-home))
  *From the 2026-10-31 UI/UX survey.*
- **[Detection] Idea: name and patronymic as a merge signal.** The Russian
  demo gives one person six aliases with a single merge hint; "Марина
  Сергеевна" next to "Павлова Марина Сергеевна" is near-certain. Would amend
  [ADR 0080](docs/adr/0080-merge-hint.md).
  ([U27](docs/evidence/ui-ux-survey/2026-10-31/README.md#ideas))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Idea: keep the Source preview next to the editor.** The
  Replacements panel sits between them; and hide Source when it has nothing
  to show. ([U28](docs/evidence/ui-ux-survey/2026-10-31/README.md#ideas))
  *From the 2026-10-31 UI/UX survey.*
- **[Import] Idea: a cleaner demo import.** The demo `.docx` uses literal
  "1." numbers and Heading 1 sections, so it imports as "1\." paragraphs and
  only H1s; `text.pdf` imports `<s>` and `<sup>` as literal tags.
  ([U29](docs/evidence/ui-ux-survey/2026-10-31/README.md#ideas))
  *From the 2026-10-31 UI/UX survey.*
- **[A11y] Idea: zoom the whole window.** View › Zoom scales only the editor;
  the sidebar, the 11–12 pt panel, dialogs and the shelf stay small.
  ([U30](docs/evidence/ui-ux-survey/2026-10-31/README.md#ideas))
  *From the 2026-10-31 UI/UX survey.*
- **[UI] Idea: a fuller tab menu.** Reveal in Finder, Copy path, Close
  others beside Save mapping as case and Close tab.
  ([U31](docs/evidence/ui-ux-survey/2026-10-31/README.md#ideas))
  *From the 2026-10-31 UI/UX survey.*

Ideas from the [2026-10-31 competitor scan](docs/design/competitor-scan-2026-10-31.md),
roughly by expected value; each needs a grill before it is promoted, and
some reopen a Deferred or ADR decision, as noted.

- **[Detection] To grill: dates of birth and other sensitive dates.** Both
  demos send a date of birth to the AI (#301); ADR 0002 leaves "ordinary
  dates" alone, and Settings has no Date category though the popup does.
  Anoni and Doczilla replace dates, Privatiser dates of birth, Tonic
  synthesizes them. Questions: detect only dates in a birth context, offer
  Date in Categories, or shift dates by one per-case offset so intervals
  survive and restore is exact. *From the 2026-10-31 competitor scan.*
- **[Detection] To grill: always and never lists.** Privatiser and Occlira
  keep user-wide words to always replace (a project codename, a deal
  amount) and never replace (the firm's own name, courts, statutes);
  Doczilla also masks sums and deal terms. mdoc decides per document or
  case. Overlaps the Deferred cross-case dictionary; decide scope (user,
  case), storage (encrypted like cases) and how the panel shows a listed
  hit. *From the 2026-10-31 competitor scan.*
- **[Detection] To grill: role-aware aliases from defined terms.**
  Contracts name parties once ("(the Employer)") and then use the role; the
  English demo replaces "Employer" as ORG_2. Deußer et al. (arXiv
  2609.11335) find unique placeholders cost the AI little while shared
  labels cost a lot; HAQQ argues role labels (`LENDER_1`) read better.
  Questions: never propose a defined term, suggest a custom alias from it
  (`EMPLOYER_1`), and measure alias styles on captured legal replies
  against ADR 0002's category tokens. *From the 2026-10-31 competitor
  scan.*
- **[Integrations] To grill: a browser companion for chat pages.** Hush,
  Caviard, Velum Lite, Privatiser and LawRider restore inside
  chatgpt.com and claude.ai (tinted real names, hover shows the alias, a
  toggle), and Hush scans attachments before upload. mdoc's round trip is
  Copy for AI, then paste the reply back. Options: an extension that asks
  mdoc (native messaging) to pseudonymize on paste and restore in page with
  the case's mapping; or keep the shelf. Trust model as for Agent access
  (Deferred). *From the 2026-10-31 competitor scan.*
- **[Integrations] To grill: Word add-in.** Occlira, Legora and LEGALFLY
  pseudonymize inside Word, Occlira with review; lawyers live in Word.
  Weigh against the Deferred DOCX export and restore-into-DOCX: an Office
  add-in talking to the local app could do both without rewriting the
  file format. *From the 2026-10-31 competitor scan.*
- **[Integrations] To grill: revisit Agent access (Deferred).** Anoni,
  Occlira (Claude Desktop connector), Velum, anonym.plus and PII Shield now
  ship MCP servers that hand agents only pseudonymized files. Decide
  whether mdoc's reviewed-text-only rule can hold over MCP (serve only
  reviewed, ready documents of a case; restore through the case) and
  whether that comes before or after the browser companion. *From the
  2026-10-31 competitor scan.*
- **[Import] To grill: e-mail import.** Anoni reads e-mail; lawyers'
  facts often arrive as `.eml` or Outlook `.msg` threads with attachments.
  Questions: headers (From/To are personal data), quoted history,
  attachments as separate tabs of one case. *From the 2026-10-31
  competitor scan.*
- **[Import] To grill: hidden content in Office files.** Hush scans DOCX
  comments, headers, footers and deleted tracked changes; ДоксДодыр strips
  metadata and revision history. Check what AnyDoc carries into the
  Markdown (comments, deleted text, document properties), then decide:
  drop, include and scan, or warn on import. A missed author name in a
  comment is a leak the panel never shows. *From the 2026-10-31 competitor
  scan.*
- **[Cases] To grill: firm-managed settings.** Doczilla lets a firm make
  pseudonymization mandatory; Harvey enforces ethical walls on every AI
  call; Velum and Anoni keep audit logs. A managed profile (macOS
  configuration profile, Windows policy) could lock categories, require
  review before Copy for AI, turn off update checks and set the model.
  Pairs with the Deferred audit journal. *From the 2026-10-31 competitor
  scan.*
- **[Restore] To grill: Russian case forms on restore.** ADRs 0041 and 0078
  decided no automatic inflection, so "с PERSON_1" restores as "с Павлова
  Марина Сергеевна". Doczilla and Amaze already match names across cases
  on detection. Revisit as a suggestion only (the inflected form shown,
  one click to accept), with a morphology dictionary, after measuring how
  often captured replies put aliases in oblique cases. *From the 2026-10-31
  competitor scan.*
- **[Detection] To grill: the next detection language.** Rivals cover
  7–46 languages and court a market each: Occlira German, Anoni and
  Lexanon French, Amaze Polish. mdoc covers EN/RU. Pick by demand with the
  Localized UI item, and price the cost: a model, identifier patterns,
  spelling, fixtures and qualification. *From the 2026-10-31 competitor
  scan.*
- **[Import] To grill: spreadsheet columns.** XLSX and CSV of clients
  (Limina and anonym.plus handle tabular data) are one name per cell;
  "replace this whole column" with one alias per distinct value would beat
  reviewing cells one by one. *From the 2026-10-31 competitor scan.*
- **[Platform] To grill: Linux.** Anoni ships macOS, Windows and Linux;
  GPUI runs on Linux. Cost: packaging, keyring (Secret Service), OCR and
  model runtimes, CI and acceptance. *From the 2026-10-31 competitor
  scan.*
- **[Product] To grill: what a paid tier would hold.** Seen: €450/yr
  (Lexanon), $159 once (Occlira), $149/yr (Velum), free plus Pro (Anoni);
  mdoc is free under the GPL. Firm features above (managed settings, Word
  add-in, audit) are what rivals charge for. Decide whether mdoc stays
  free, sells support, or offers a firm edition. *From the 2026-10-31
  competitor scan.*

Mobile ideas from the [2026-10-31 mobile UX ideas](docs/design/mobile-ux-ideas-2026-10-31.md)
(the iOS survey's after set, the app's code and a scan of mobile
redaction apps), roughly by expected value; each needs a grill, and
Android takes what applies. Its clipboard finding is
[#321](https://github.com/lawkitt/mdoc/issues/321).

- **[Mobile] To grill: app switcher cover and an optional Face ID lock.**
  The app switcher's snapshot shows client names; a plain cover when the
  scene leaves the foreground, and an opt-in lock for the app and Restore
  with mdoc (Android: `FLAG_SECURE`, biometric lock). *From the 2026-10-31
  mobile UX ideas.*
- **[Mobile] To grill: one proposal at a time.** A full-screen review card
  (the proposal in its sentence, category, alias), Replace and Keep at
  thumb height or by swipe, "3 of 12", haptics, undo the last; today the
  candidate sheet covers half the text. *From the 2026-10-31 mobile UX
  ideas.*
- **[Mobile] To grill: undo.** Replace, Keep, Replace all, Keep all,
  Remove reply and Delete document have no undo on iOS; an Undo toast plus
  `UndoManager` (shake, three-finger swipe). *From the 2026-10-31 mobile UX
  ideas.*
- **[Mobile] To grill: merge hint and Same as… on mobile.** ADR 0080's hint
  on Identities rows and Same as… in the candidate sheet; without them
  "Ms Bennett", "Maya" and "Mia" reach the AI as three people. *From the
  2026-10-31 mobile UX ideas.*
- **[Mobile] To grill: "As the AI sees it".** A read-only view of the Copy
  for AI text with any original still in it highlighted, so the user sees
  what leaves before sharing; it could also be the default while the screen
  is recorded or mirrored. Desktop has no such view either. *From the
  2026-10-31 mobile UX ideas.*
- **[Mobile] To grill: text from any app becomes a document.** Shared text,
  web pages and Mail selections open as a pasted document (the extension
  hands over; the app does the work); Android's `ACTION_PROCESS_TEXT` puts
  mdoc in every app's selection menu. Today only files arrive by share.
  *From the 2026-10-31 mobile UX ideas.*
- **[Mobile] To grill: a demo document before the download.** "Try a
  sample" on the empty Home: the settlement letter with stored proposals,
  so review, share and restore work before the 649.5 MB download. *From
  the 2026-10-31 mobile UX ideas.*
- **[Mobile] To grill: Shortcuts and Siri.** App Intents for Restore text
  with a case, Get a reviewed document's text for AI, and Add text to mdoc
  (opens for review), so iOS 26's Use Model action can chain share, ask and
  restore; no intent hides names without review. *From the 2026-10-31
  mobile UX ideas.*
- **[Mobile] To grill: find, and mark every match.** `UIFindInteraction`
  (⌘F on iPad) and Replace all matches as one identity, for a surname the
  detector missed. *From the 2026-10-31 mobile UX ideas.*
- **[Mobile] To grill: share a selection.** "Share for AI" in the selection
  menu sends one clause through Copy for AI's export path. *From the
  2026-10-31 mobile UX ideas.*
- **[Mobile] To grill: remove documents after a while.** Settings: never, 7,
  30 or 90 days; documents and replies go, cases stay so replies still
  restore; storage per kind shown. *From the 2026-10-31 mobile UX ideas.*
- **[Mobile] To grill: iPad as a work surface.** Drop files in, drag the
  text for AI out to an AI app in Split View and the reply back in, keyboard
  shortcuts (⌘F, R/K, arrows), two windows. *From the 2026-10-31 mobile UX
  ideas.*
- **[Mobile] To grill: highlights on the original page.** Found names boxed
  on scan and photo pages, if OCR keeps word boxes. *From the 2026-10-31
  mobile UX ideas.*
- **[Mobile] To grill: hints, haptics and quick actions.** TipKit for
  select-to-mark, long-press Share and the Restore extension; haptics on
  decisions; Home Screen quick actions Scan, Paste, Paste reply. *From the
  2026-10-31 mobile UX ideas.*

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
