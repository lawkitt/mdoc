# Roadmap

mdoc prepares local documents as editable Markdown for external AI tools.
This roadmap lists open work as of 2026-11-01 (latest: 0.3.0); accepted
behavior lives in [ADRs](docs/adr/), with terminology in [CONTEXT.md](CONTEXT.md).
How it is kept: [ADR 0059](docs/adr/0059-roadmap-after-0-1-0.md) and
[Roadmap upkeep](docs/development.md#roadmap-upkeep). Bugs go to
[Issues](https://github.com/lawkitt/mdoc/issues).

## Current status

0.3.0 shipped on 2026-11-01 for Apple Silicon macOS and Windows x64: cases
restore without surprises (restore in place, merge hints, What you did,
Back and Forward), the macOS menu bar shelf, and the 2026-10-31 UI/UX
survey's fixes and polish. It went stable from rc.1 with no rc.2 and no
acceptance run, an exception to
[ADR 0083](docs/adr/0083-fewer-release-candidates.md) recorded in the
[2026-11-01 grill](docs/design/roadmap-grill-2026-11-01.md); a targeted run
on the release build follows (0.3.x). No PII model has passed the
[initial qualification](tests/fixtures/pseudonymization/README.md). Earlier
releases: [CHANGELOG](CHANGELOG.md).

Three tracks run in parallel and each releases when ready: desktop 0.4.0,
iOS (the first to ship) and Android.

## 0.3.x

A severe bug in 0.3.0 gets a patch; other bugs go to the
[0.4.0 milestone](https://github.com/lawkitt/mdoc/milestone/7).

1. **[Release] 0.3.0 acceptance on the release build.** *From the
   2026-11-01 grill.* 0.3.0 skipped rc.2's acceptance, so the shelf has
   never had its section run. Run [platform acceptance](docs/platform-acceptance.md)
   section 12, the GUI rows on #254 and #353, and an install, upgrade and
   keychain smoke on both platforms, on the v0.3.0 release build. *Done
   when:* the evidence is under `docs/evidence/platform/`, #254 and #353
   are closed, and each must-fix fail is an issue in a 0.3.1 milestone.

## 0.4.0

Fixing PII recognition flaws on the fly
([milestone](https://github.com/lawkitt/mdoc/milestone/7)). The pinned
model misses things no threshold recovers: neither demo's home address is
proposed at any score and it has no date-of-birth label; a lone "Марина"
scores 0.29, under the 0.5 cut ([probe](docs/design/roadmap-grill-2026-11-01.md#probe-what-the-model-sees-in-the-demos)).
A correction pipeline after the model fixes them in layers
([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)), and a flaw
hunt loop finds, records and fixes them with the maintainer away
([ADR 0093](docs/adr/0093-flaw-hunt-loop.md)): Codex hunters, an Opus
curator and Opus fixers, a strict fixture ratchet and a private holdout.
Items 3 and 4 run as loops once item 2 merges. rc.1, then full acceptance
on the last RC; no stable without it ([ADR 0083](docs/adr/0083-fewer-release-candidates.md)).
Records: [roadmap grill](docs/design/roadmap-grill-2026-11-01.md),
[recognition grill](docs/design/recognition-grill-2026-11-01.md).

3. **[Detection] Rule pack.** *From the 2026-11-01 grills (#301).* ADR
   0092's TOML pack replaces `mdoc-detect`'s hardcoded lists and
   `guess_category`'s shapes; personal-context dates, the Case number
   category (`CASE_1`) in Settings and the popup, anchored EN and RU addresses,
   `never` for titles, roles and defined terms, HTML entities decoded
   before detection and readiness. Groundwork (loader, lists moved, no
   behaviour change) starts beside item 2; then a fix loop on hunted flaw
   classes. *Done when (amended by the
   [run 3 grill](docs/design/recognition-run3-grill-2026-11-09.md),
   [ADR 0108](docs/adr/0108-recognition-targets.md)):* both demos' Copy
   for AI hold no address, date of birth or case number, and the real set
   meets ADR 0108's targets (recall at least 95% overall and 90% per
   leaking category, precision at least 85%) with the holdout at or above
   97.6% recall and 94.0% precision. Miss classes block; false-hit classes
   below ADR 0108's volume line wait in the Recognition backlog.
4. **[Detection] Propagation in the document.** *From the 2026-11-01
   grills (U27).* ADR 0092's propagation: an applied or marked name's
   forms (surname or first name alone, first name + patronymic, surname
   with initials, inflections, nicknames) become proposals linked to it
   when the candidate is unique. *Done when:* in the Russian demo,
   Павлова's six aliases become one and "Марина" links, and item 3's
   targets hold ([ADR 0108](docs/adr/0108-recognition-targets.md)).
   Run 4 (one Windows hub) works items 3 and 4 together; a model spike
   beside it informs 0.5, not rc.1.
The 2026-10-31 survey's polish (U13, U18, U19, U23, U24, U26 in Later)
runs as a separate overnight hub during 0.4.0, outside the item count.

## iOS

The review-first iPhone and iPad app on the shared Rust core
([ADR 0085](docs/adr/0085-mobile-apps.md)). One section for every iOS
version: an item names the version it ships in when that matters. Core-crate
changes merge when green and leave desktop unchanged. Built so far: 0.1's
app (#266–#278, [mobile grill](docs/design/mobile-grill-2026-10-31.md)) and
0.2's review bar, UI system, theme, Settings, iPad keys and Try a sample
(#604–#609, [ADR 0098](docs/adr/0098-mobile-review-bar.md),
[ADR 0099](docs/adr/0099-mobile-ui-system.md),
[review grill](docs/design/ios-review-grill-2026-11-05.md)). 0.1 is not
released on its own: the next TestFlight build is `ios-v0.2.0-rc.3` and
runs 0.1's acceptance rows too
([refactor grill](docs/design/ios-refactor-grill-2026-11-07.md) Q24).
Features are frozen until items 13–19 land (Q4).

1. **[Mobile] TestFlight and acceptance.** *From the iOS build-out
   (#269, #272, #275–#278) and the
   [TestFlight grill](docs/design/testflight-grill-2026-11-03.md)
   ([ADR 0097](docs/adr/0097-ios-testflight-beta.md)).* The app record
   exists ("mdoc: private docs for AI"), the upload's pieces are merged
   (#578), and `just ios-device-test` passes on the iPhone 17 Pro on
   cellular and the 12 Pro on Wi-Fi (#583, #584): the maintainer kept the
   17 Pro on cellular (slow Wi-Fi), so each download path ran on one phone
   instead of the 17 Pro on Wi-Fi. P17 passed on the 17 Pro after its fixes
   (#591; a locked phone pauses a job, accepted). Builds `ios-v0.1.0-rc.2`
   (734) and `rc.3` (742) went to the internal group (`rc.1`'s upload
   failed on the export key, now NO). Section 13's Simulator rows and P12
   ran on 2026-11-04 ([evidence](docs/evidence/mobile/2026-11-04/ios.md)):
   all pass except P13, a Later item. Left: the Device rows, and P18's
   device half, by hand on `ios-v0.2.0-rc.3` (`rc.1` and `rc.2` were
   tagged on 2026-11-05, builds 750 and 751).
   *Done when:* section 13 passes on a TestFlight build.
8. **[Mobile] Device check and TestFlight.** The maintainer checks the
   review bar, undo, theme, Settings, the sidebar and Identities on the
   iPhone 17 Pro from `ios-v0.2.0-rc.3`, after items 13–19. Items 10–12 get
   a first look from a Release dev build right after they merge (Q3).
   *Done when:* that build passes the check, or its findings are `ui`
   issues.
10. **[Mobile] The review bar follows the finger.** The drag, the full
    view's mention rows, Keep instead and Next, hint lines in place of
    TipKit, All reviewed ([ADR 0101](docs/adr/0101-mobile-review-bar-polish.md),
    #659). *Done when:* merged and checked on the iPhone 17 Pro.
11. **[Mobile] The sidebar.** Claude-style sidebar over the open document,
    Start, Cases and Documents pages, ⌘K and ⌘N
    ([ADR 0102](docs/adr/0102-mobile-sidebar.md)). *Done when:* merged and
    checked on the device.
12. **[Mobile] Identities as a drawer.** The right-edge drawer, ordered by
    what is left, state buttons, Replace N, Looks like, Select
    ([ADR 0103](docs/adr/0103-mobile-identities-drawer.md)). *Done when:*
    merged and checked on the device.
20. **[Mobile] To grill: peek at the originals.** *From the
    [sidebar grill](docs/design/ios-sidebar-grill-2026-11-06.md) ideas and
    Q7.* Press and hold to see the originals in place of the aliases;
    "As the AI sees it" (Later) folds into the same control. *Done when:*
    grilled and built, after item 19.
21. **[Mobile] To grill: Paste a reply from Start.** *From the sidebar
    grill ideas and Q7.* After sharing, Start offers to restore a reply
    whose aliases match the case, through the system paste button, so
    there's no paste prompt. *Done when:* grilled and built, after item 19.
22. **[Mobile] To grill: a case's page.** *From the sidebar grill ideas.*
    A case's names, documents and replies (desktop's ADR 0077), with Delete
    a case (Later) on it. *Done when:* grilled and built, after 20 and 21.

23. **[Mobile] Editing (iOS 0.3).** *From the
    [mobile editor grill](docs/design/mobile-editor-grill-2026-11-09.md)
    ([ADR 0109](docs/adr/0109-mobile-markdown-editor.md)).* Edit any
    document in an Obsidian-like edit mode on a shared Rust editor core,
    after items 13–19. Slices, one PR each: (1) the editor core extracted
    from `mdoc-editor`, desktop switched with no behaviour change; (2) edit
    mode with Show Markdown on: autosave, undo, proposals following edits,
    in-app clipboard, paste, find; (3) live-preview editing: reveal rule,
    caret over hidden markers, incremental restyle; (4) toolbar,
    formatting keyboard, links; (5) tables; (6) new document, Find names
    again, iPad keys. Device checks after slices 3 and 6. *Done when:*
    ADR 0109's acceptance bars pass and the iPhone 17 Pro and 12 Pro
    checks have no open `ui` findings.

25. **[Mobile] Menu, swipes and a name's words.** *From the
    [iOS UI grill](docs/design/ios-ui-grill-2026-11-13.md)
    ([ADR 0114](docs/adr/0114-ios-menu-swipes-and-name-words.md)).* Slices,
    one PR each: (A) the edit-mode crash in a large table document and the
    jump to its end, first; (B) one document menu, the Identities header and
    the Select demo, hints moved onto TipKit; (C) document-wide drawer swipes
    on a UIKit pan, frame-checked on the 12 Pro; (D) the Hide name callout
    and Fix the words with its core action; (F) the review card's title
    wraps, quieter row glyphs. B and C in parallel after A, then D and F;
    TestFlight rc.10 after. *Done when:* all merged and checked on the
    iPhone 17 Pro from rc.10.

## Android

The review-first Android app on the shared Rust core, built beside iOS
on the Windows PC ([ADR 0089](docs/adr/0089-android-app.md)). Core-crate
changes merge when green and leave desktop and iOS unchanged. One section
for every Android version. It follows [ADR 0104](docs/adr/0104-mobile-app-structure.md)
when it next moves (refactor grill Q25). Record:
[Android grill](docs/design/android-grill-2026-10-31.md), whose
Maintainer actions list the steps on the maintainer's side.

2. **[Android] Feasibility spike.** `tools/android-spike` and
   `scripts/android-spike-device.sh` on Test Lab phones at 6, 8 and 12 GB
   or more. *Measured 2026-10-31
   ([report](docs/evidence/mobile/2026-10-31/README.md#android-adr-0089)):*
   memory, load, heat and OCR pass on all four phones, but FP16 on the CPU
   scans 3.6 to 9 times slower than the iPhone 17 Pro and misses the 60 s
   budget everywhere; 4 threads is already fastest and XNNPACK can't load
   the export (#349). Left: FP32, INT8 and big-core pinning on the Pixel 8
   and Galaxy A55. *Stop rule (2026-11-01 grill):* if none fits 60 s,
   Android 0.1 ships rules-only (identifiers, OCR, marking names by hand),
   as iOS does below its floor, and Find names on Android goes to Later.
   *Done when:* each row has pass or fail against ADR 0085's budgets and
   Android's floor constant is set from them.
3. **[Android] Device runs (items 3, 5 and 7).** *From #314 and #322.* The
   scaffold, the Android job, the long-job test and `just
   android-device-test` are merged; the Windows runner needs reinstalling
   to run the Android job, and nothing has run on Test Lab yet (quota).
   *Done when:* a PR touching `apps/android` gets a green `local/android`
   from the Android job; `just android-device-test` passes on one Test Lab
   phone per RAM class; and the long jobs (import, OCR, Find names above
   the floor) survive leaving the app and a process kill on a Test Lab
   Pixel and Samsung phone with no crash or ANR, and a model download does
   on the friend's phone.
8. **[Android] Play closed test.** The Play account, the device check and
   12 testers. *Done when:* the closed test has had 12 testers opted in for
   14 days in a row.
9. **[Android] Acceptance and `android-v0.1.0`.** Platform acceptance
   section 14 (A1–A17). *Done when:* the section passes on the release
   build, the signed APK and its SHA-256 are on GitHub Releases, and the
   Play production release is submitted (or the slip is recorded).

10. **[Android] Editing.** *From the mobile editor grill Q2.* The editor
    screen on the editor core ADR 0109 adds, after Android's rendered
    Markdown (#569) and iOS item 23's slice 1. *Done when:* grilled for
    Compose specifics and built to ADR 0109's rules.

## Mobile CI/CD

Mobile tests stay on local CI with a real fallback, and agents release the
apps to internal tracks and drafts
([ADR 0091](docs/adr/0091-mobile-ci-and-releases.md)). [ADR 0095](docs/adr/0095-ci-on-a-merge-queue.md)
moves the iOS job and the Android fallback to hosted runners (CI redesign,
below). Record:
[mobile CI grill](docs/design/mobile-ci-grill-2026-11-01.md). Items 2 and 3
touch separate files and run as parallel lanes; 5 builds on both.

5. **[CI] Dependabot and Gradle metadata.** Dependabot and fork PRs run
   the hosted jobs of their areas; a dispatched workflow regenerates Gradle
   verification metadata as a reviewable diff. The Mac takeover is dropped
   and the hosted mobile jobs move to CI redesign item 2
   ([ADR 0095](docs/adr/0095-ci-on-a-merge-queue.md)). *Done when:* a
   Gradle Dependabot PR goes green after its metadata commit.
6. **[Release] iOS release workflow.** *From the
   [agents TestFlight grill](docs/design/ios-testflight-agents-grill-2026-11-08.md)
   ([ADR 0107](docs/adr/0107-agents-publish-ios-betas.md)).* The key is in
   `release-ios` (`ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8`, body only).
   One PR: `ios-release.yml` on `ios-v*` tags (checks, `ci-ios.yml` on the
   tag, a full-history archive with Apple-managed signing, upload, the
   processing wait, What to Test from the tag's annotation, a run summary)
   with a validate mode dispatched from `main`; `just release-ios
   <x.y.z>-rc.N <file>`, which tags the gated `main` commit with no commit
   of its own; a shell test of the recipe's checks. Codex reviews it.
   *Done when:* a validate run from `main` passes and `ios-v0.2.0-rc.3`
   reaches "Maintainer" from the workflow with no maintainer step.
7. **[Release] Android release workflow.** After the Play account (Android
   item 8): `just release-android`, `android-v*` tags, the minified variant
   on the hosted emulator, the AAB to Play's internal track, Play's signed
   universal APK in a draft GitHub Release with SHA256SUMS and provenance,
   notes from `changes/android/`. *Done when:* an agent produces that draft
   with no maintainer step.

## CI redesign

`main` merges through GitHub's merge queue; PR heads run a fast tier, and
the full suite runs once per queue group on hosted runners and the PC
([ADR 0095](docs/adr/0095-ci-on-a-merge-queue.md), cut over 2026-11-03).
Record: [CI redesign grill](docs/design/ci-redesign-grill-2026-11-02.md);
items 8–15: [ADR 0100](docs/adr/0100-ci-reliability-after-the-cutover.md),
[CI fixes grill](docs/design/ci-fixes-grill-2026-11-06.md).

7. **[CI] `main`, supply chain and upkeep.** The detection shards back to
   10 minutes once `main-caches.yml`'s caches make a warm shard fit; a
   nightly full run of every hosted job, the quarantined tier included,
   opens an issue when red; cargo-deny advisories move to
   `Cargo.lock` PRs and a daily run that opens an issue; the local CI
   scripts (`local-ci.sh`, `local-ci-fallback.sh`, `local-ci-setup.ps1`)
   go, their area checks kept. *Done when:* a
   nightly drill with a broken fallback job opens an issue.

15. **[CI] CI targets.** A measurement with `scripts/ci-stats.sh --since
    2026-11-06T00:35:00Z` once a day of queue traffic has run on items
    8–14, recorded in ADR 0100 (the script is #627; the parallel-testing
    verdict, off, is in #617). *Done when:* the measurement is recorded.

## CI speed

The queue's long jobs get shorter and its unrelated drops stop needing the
maintainer ([ADR 0112](docs/adr/0112-ci-speed-after-the-first-fortnight.md),
[CI speed grill](docs/design/ci-speed-grill-2026-11-12.md)) and the
[second grill](docs/design/ci-grill-2-2026-11-13.md)
([ADR 0113](docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
Items 16–31 have shipped; item 22 (the PC's slow bursts) closed as not
needed, and item 30 (a prebuilt library for UI test launches) was measured
and dropped (#857).

## Later

One line each, in rough order; promoted and written out at grooming. Earlier
detail: [ROADMAP.md before ADR 0059](https://github.com/lawkitt/mdoc/blob/c400853/ROADMAP.md#after-010).

- **[Perf]** Consider gpui-fast (longbridge's GPUI fork with frame-to-frame
  caching) for the app's per-frame CPU. Costs a newer GPUI and porting the
  three vendored patches ([CI speed grill](docs/design/ci-speed-grill-2026-11-12.md)).

- **[Recognition]** Real-text fixtures: commit public real documents as
  fixtures, each with a licence check, which loosens ADR 0093's CC0 rule.
  *From the 2026-11-02 grill (#471).*
- **[Recognition]** A real-document eval set: a private, annotated holdout of
  public real documents that scores recognition on real text and is never
  in a fixer's brief. *From the 2026-11-02 grill (#471).*
- **[OCR]** OS engines (Apple Vision, `Windows.Media.Ocr`) as the no-download
  default where they beat PP-OCR on our fixtures by character error rate; Vision
  reads Russian from macOS 13, Windows needs the Russian pack. *From Panoply.*
- **[Perf]** Stress tier (2 MiB / 20,000 mentions) and numerical budgets.
- **[Editor]** To grill: Fix the words on desktop. The core action that
  replaces a found name's wording everywhere, keeping its identity, given
  desktop's own UI. *From the iOS UI grill Q10 (ADR 0114).*
- **[Editor]** To grill: desktop's reveal rule. Whether desktop adopts the
  phone's (the span under the caret shows its markers), after the phone has
  been used. *From the mobile editor grill Q12.*
- **[Mobile]** Save reply as document: make a restored AI reply an editable
  document of its own. *From the mobile editor grill Q23.*
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
- **[Mobile]** US spelling in the Android app: desktop moved every UI
  string to US spelling (UI polish decision 8, #327), and iOS followed in
  the 2026-11-04 journey polish, but Android still says "unrecognised" and
  "Licence". *From the 2026-11-01 UI polish run.*
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
- **[Release]** Release runs reuse `main`'s CI: the `version` job skips the
  called CI when the release commit only bumps versions and the CHANGELOG
  and `main` passed its parent; `smoke-package` adds the Russian fixture
  ([ADR 0083](docs/adr/0083-fewer-release-candidates.md)). With one RC per
  minor it saves about 20 minutes a release. *From the 2026-10-30 grill;
  moved from Engineering at the 2026-11-01 grill.*

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
- **[UI] Polish: Pseudonymize clipboard on Home looks broken.** It jumps to
  the last document tab and replaces only that document's names, so new text
  comes back "unchanged". Say on Home which names it uses, or let it pick a
  source as the shelf does.
  ([U26](docs/evidence/ui-ux-survey/2026-10-31/README.md#u26-pseudonymize-clipboard-from-home))
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

Ideas from the [2026-10-31 competitor scan](docs/design/competitor-scan-2026-10-31.md),
roughly by expected value; each needs a grill before it is promoted, and
some reopen a Deferred or ADR decision, as noted. Dates of birth, always and
never lists, role-aware aliases and hidden Office content moved to 0.4.0.

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

- **[Android] The review bar and the UI system.** ADR 0098 and 0099 on
  Android with Material conventions: the bar, undo through the shared
  core, the identity tools, tokens under `ui/theme`, theme, Settings and
  Home. *From the 2026-11-05 review grill.*

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

- **[Mobile] To grill: Shortcuts and Siri.** App Intents for Restore text
  with a case, Get a reviewed document's text for AI, and Add text to mdoc
  (opens for review), so iOS 26's Use Model action can chain share, ask and
  restore; no intent hides names without review. *From the 2026-10-31
  mobile UX ideas.*

- **[Mobile] To grill: share a selection.** "Share for AI" in the selection
  menu sends one clause through Copy for AI's export path. *From the
  2026-10-31 mobile UX ideas.*
- **[Mobile] To grill: remove documents after a while.** Settings: never, 7,
  30 or 90 days; documents and replies go, cases stay so replies still
  restore; storage per kind shown. *From the 2026-10-31 mobile UX ideas.*
- **[Mobile] To grill: iPad as a work surface.** Drop files in, drag the
  text for AI out to an AI app in Split View and the reply back in, two
  windows (review keys are built). *From the 2026-10-31 mobile UX
  ideas.*
- **[Mobile] The Original at the caret's page.** *From the editor polish
  grill Q10.* For a scan, the Original opens at the page the caret is on,
  from the text's page headings.
- **[Mobile] A heading that does not shift.** *From the editor polish grill
  Q18.* The heading moves when the caret shows its `#`; `##` and `###` do
  not fit the margin.
- **[Mobile] To grill: highlights on the original page.** Found names boxed
  on scan and photo pages, if OCR keeps word boxes. *From the 2026-10-31
  mobile UX ideas.*
- **[Recognition] To grill: hint transliterated pairs.** The same name in
  Latin and Cyrillic letters ("Ivanov" and "Иванов") comes out as two
  identities, and merge hints treat transliteration as no signal on
  purpose; a hint would change desktop too. Mobile's Select and Same as fix
  a pair by hand meanwhile. *From the 2026-11-06 sidebar grill (Q13).*
- **[Mobile] Delete a case.** From the Cases page, confirmed and
  irreversible as on desktop (ADR 0041 Forgetting); cases a move left
  empty stay until then. *From the 2026-11-06 sidebar grill (#660
  review).*
- **[Mobile] To grill: pinned cases.** Pin a case to the top of the
  sidebar, as Claude pins chats. *From the 2026-11-06 sidebar grill.*
- **[Mobile] To grill: quick actions.** Home Screen quick actions Scan,
  Paste, Paste reply (tips and haptics are built). *From the 2026-10-31
  mobile UX ideas.*
- **[Desktop] To grill: an ink accent.** iOS moved from the teal to ink
  (ADR 0099 as amended 2026-11-08); desktop's focus rings, toggles and
  selection need their own look before it follows. *From the 2026-11-08
  [gestures grill](docs/design/ios-identities-gestures-grill-2026-11-08.md) Q9.*
- **[Mobile] To grill: kept names before sharing.** One line above Copy
  for AI, "Sharing with 2 names kept as they are: …", listing them on a tap,
  so an unmeant Keep shows at the step that can't be undone. *From the
  [refactor grill](docs/design/ios-refactor-grill-2026-11-07.md) Q7.*
- **[Mobile] To grill: long jobs in the Dynamic Island.** A Live Activity
  for OCR and Find names ("Recognizing page 3 of 12"), no title (ADR 0085's
  invariant). Needs a widget extension. *From the sidebar grill ideas.*
- **[Mobile] To grill: a widget.** "3 documents to review", opening the
  first; counts only, no titles. *From the sidebar grill ideas.*
- **[Mobile] To grill: document thumbnails.** The first page of a PDF or
  scan on the Documents page; the cache stays out of backups. *From the
  sidebar grill ideas.*

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
- **GUI checks without a takeover** (fleet ADR 0019): a debug-only switch
  (`MDOC_KEEP_RENDERING=1`) in the vendored `gpui-pre-macos` keeps the display
  link running while the window is covered, so the hub's background captures
  stay live. *Done when:* an experiment shows covered captures update and
  whether background keys reach GPUI, and `docs/development.md` (Fleet) says
  which GUI rows still need focus. *From the 0.3.0 away hub.*
- **Dev-build profile override:** debug builds read `MDOC_PROFILE_DIR`, so GUI
  checks run on a scratch profile instead of moving the user's
  `session.json` and `recents.json` aside. *From the 0.3.0 away hub.*

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
