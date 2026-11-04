# iOS acceptance, build 1 (2026-11-04)

Section 13 of [platform acceptance](../../../platform-acceptance.md) for
build 1, `ios-v0.1.0-rc.2` (0.1.0, build 734, `e03927ee`)
([ADR 0097](../../../adr/0097-ios-testflight-beta.md)). The maintainer
installed build 734 from TestFlight on the iPhone 17 Pro, and it launches.
An agent ran the *Simulator* rows and P12. A TestFlight build can't be
installed in the Simulator, so those rows ran on a Simulator build of the
same tag (`target/ios/DerivedData`, version 0.1.0 (734)). The *Device* rows
are the maintainer's and are still pending.

**Simulator rows:** 8 Pass, 1 Fail (P13), 1 partly done (P18, device half
pending). **Device rows:** pending.

## Header

| Field | Value |
| --- | --- |
| Date | 2026-11-04 |
| Run by | Claude (Simulator rows, P12); the maintainer (Device rows, pending) |
| mdoc version and artifact | 0.1.0 (734), `ios-v0.1.0-rc.2` at `e03927ee`; TestFlight build 734 on the iPhone 17 Pro; Debug Simulator build of the same tag for the Simulator rows |
| Machine, CPU, RAM, GPU | MacBook Air, Apple M4, 24 GB |
| OS and build | macOS 15.8.1 (24H32), Xcode 26.1.1, iOS Simulator 26.1 |
| Displays and scale | Simulators: iPhone 17 Pro (survey, tests, rendering), iPhone 17 Pro Max (erased, for P1, P2 and P6), iPad Pro 13-inch (M5) (P12) |
| Input sources installed | Not used |

## How the Simulator rows ran

- `scripts/ios-survey.sh` on the iPhone 17 Pro and the iPad Pro 13-inch
  (M5): 60 screenshots each, light and dark, all steps passed.
- The full `xcodebuild test` (unit, parity and UI tests, quarantine applied)
  on the iPhone 17 Pro: 150 tests, 133 passed, 17 skipped (the opt-in
  survey, device, audit and rendering tests), 0 failed.
- `RenderedMarkdownUITests` (`TEST_RUNNER_MDOC_RENDERING=1`) three times:
  light at the default size, dark, and light at the largest text size
  (AX XXXL, set with `simctl ui`). All passed.
- `LargeTextExploreUITests` with `TEST_RUNNER_MDOC_SHOTS_DIR`: the main
  screens at AX XXXL.
- On a freshly erased iPhone 17 Pro Max: install, first launch, `lsof` on
  the app's process, the App Group folder's backup flags, and the Files app
  driven by computer use with `tests/fixtures/reference.pdf`,
  `tests/fixtures/docx-preview/legal-opinion.docx` and a PNG made from
  `reference.pdf`'s page.

## 13. iOS app

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| P1 | *Simulator*: first launch shows Home with no network, Keychain or permission prompt; Settings › About shows the version, the GPL and app-store permission notice and the source link | Pass | Erased iPhone 17 Pro Max: Home with the setup card, no prompt. The app had no socket open from launch through the next 20 s (`lsof`, 586 polls). About shows 0.1.0 (734), core commit `e03927e`, the GPL text, the GPL section 7 app-store permission with its link, and Source code. |
| P2 | *Simulator*: Settings says "mdoc's documents stay on this iPhone and are not in backups."; the app container's documents, cases and models carry `isExcludedFromBackup` | Pass | Settings shows the sentence (iPad: "on this iPad"). After three imports and a model download, the App Group's `mdoc` folder and each area in it (documents, cases, models, originals, imports, downloads, hand-off) carry the backup-exclusion flag. The app's own data container holds only system files. |
| P3 | *Device*: the setup card names the model size; on cellular it asks before downloading; leaving the app keeps the download going; a corrupted file fails verification and is not used | | Pending: maintainer |
| P4 | *Device*: on a phone below the device floor, setup offers only text recognition; where Find names would be, the document says the phone cannot find names, why, and that selecting text marks one, and Settings says the same (ADR 0088) | | Pending: iPhone 12 Pro run. The below-floor survey screens (31–33) show the wording on iPhone and iPad. |
| P5 | *Device*: Scan (document camera) of a printed fixture page recognizes English and Russian text with the original page beside it | | Pending: maintainer |
| P6 | *Simulator*: Files › PDF, DOCX and image import; "Open in mdoc" from the Files app's share sheet opens the document | Pass | + › Files imported `reference.pdf` with its text and Text/Original. The PNG imported with "No text yet" and the text recognition card; after the 18 MB download its text was read. Files › `legal-opinion.docx` › Share › mdoc opened Add document, then the document, ready to find names. The Simulator's network counts as not Wi-Fi, so mdoc asked before downloading; P3 checks the real path on the phone. |
| P7 | *Simulator*: candidates are underlined; tapping one opens the sheet with its alias; Keep, Replace, All occurrences and Just this one act as on desktop; the Identities sheet lists identities by category | Pass | Survey 12–17: the email is underlined, its sheet shows Found, Category, Replace/Keep and the alias `EMAIL_1`; Replace 1 replaces it; Identities lists it under Email (1) as Replaced. The decision scopes are covered by `RealCoreTests` and `FakeCoreTests` (All occurrences, Just this one), which passed. |
| P8 | *Simulator*: selecting missed text and choosing Replace… adds it to the review | Pass | Survey 18–19: a selection offers Replace…, whose sheet shows the selected text and a category. |
| P9 | *Device*: Share to the ChatGPT or Claude app sends aliases only, with the placeholder note; Copy puts the same bytes as desktop Copy for AI | | Pending: maintainer. The Copy-for-AI parity tests passed in the Simulator run. |
| P10 | *Device*: "Restore with mdoc" from the AI app's share sheet restores the names of the last shared document's case, the picker switches cases, and Save to document stores the reply | | Pending: maintainer |
| P11 | *Device*: a 20-page scan finishes within ADR 0085's budget; leaving the app keeps it running with system progress, and if iOS refuses or ends that, the app offers to resume on return | | Pending: maintainer. `just ios-device-test` measured 6% → 73% while away on this code (2026-11-04). |
| P12 | *iPad*: Text and Original sit side by side and the Identities list sits beside the text | Pass | Simulator only (iPad Pro 13-inch (M5)); no real iPad. The iPad survey shows the PDF's Text and Original side by side with Identities beside them, and Identities beside the text in the review, reply and Markdown screens, light and dark. |
| P13 | *Simulator*: light and dark mode, and the largest Dynamic Type size, keep every screen readable with nothing clipped | Fail | Light and dark: every survey screen on iPhone and iPad reads fully. At the largest size, Settings' Download button breaks its word and is cut off: "Down- / load 18…" ([screenshot](p13-settings-largest-text.png)). The rest is readable, though the stacked Share, Copy and Paste bar takes about a third of the screen. Not must-fix (no data loss; the button still works): ROADMAP Later, "[Mobile] Settings' Download button at the largest text size". |
| P14 | *Device*: VoiceOver reads each underlined candidate with its alias, and its actions (Keep, Replace) work from the rotor; the sheets announce their title and buttons | | Pending: maintainer |
| P15 | *Device*: after locking the phone, documents and cases are unreadable to a backup or file browser (Data Protection Complete); a restored backup on another phone starts empty | | Pending: maintainer |
| P16 | *Simulator*: at every text size no word in the review text, Identities or a restored reply is hyphenated, and in dark mode every candidate underline reaches 3:1 contrast with stronger fills than in light (ADR 0087) | Pass | No hyphen in the review text, Identities, the candidate sheet or the reply sheet at AX XXXL or in the survey. `testRenderedParagraphsNeverHyphenate` and `testDarkHighlightsStandOut` (3:1, stronger dark fills) passed. At the largest size, a word wider than the line breaks without a hyphen ("Уведомлени / е" in a heading). Settings' hyphen is outside this row and is P13's Fail. |
| P17 | *Device*: on each phone at or above the detector floor, from a fresh install: the models download on Wi-Fi (and the job finishes after leaving the app), a .docx, a text PDF, a scanned PDF, a camera scan and pasted text come in, Find names underlines names, Replace from a selection, one-tap Share and its Markdown file, Restore with mdoc and a pasted reply, and a lock during Find names; no new `mdoc-*.ips` crash report. Below the floor: the document and Settings say the phone cannot find names (ADR 0088) | | Pending: a short pass on the TestFlight build. The full run passed on a development build of this code after #591. |
| P18 | *Simulator and device*: each rendering survey document (`tests/fixtures/mobile-markdown/rendering`, `TEST_RUNNER_MDOC_RENDERING=1` runs `RenderedMarkdownUITests`) reads as desktop draws it, light and dark and at the largest text size: no Markdown marker shows, tables are grids whose long cells wrap without breaking a word and scroll sideways, lists carry desktop's bullets and `1.` → `a.` → `i.` numbers with a hanging indent, alerts, quotes, code and rules are drawn; a name in a table cell opens its sheet and Replace… works in a cell; Show Markdown shows the source and back (ADR 0096) | | Simulator half Pass: all six documents in light, dark and AX XXXL show no marker. Tables are grids that scroll sideways with wrapped cells, lists carry desktop's labels, and the alert, quote, code and checkbox blocks are drawn. Pending on the device: a name in a table cell, Replace… in a cell and Show Markdown (the Simulator has no detector, so the survey documents had no names). |
