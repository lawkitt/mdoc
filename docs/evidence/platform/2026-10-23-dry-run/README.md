# Platform acceptance dry run, 0.2.0 (2026-10-23)

A macOS dry run of the 0.2.0 acceptance checklist
([ADR 0057](../../../adr/0057-native-platform-acceptance.md), template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)), run before
0.2.0 has a release candidate to find failures early. **It checks a local
build of `main`, not the CI RC**, so it does not count as the 0.2.0
acceptance run (ROADMAP 0.2.0 must-fix 1). It includes S1 and A5, deferred
from 0.1.0, and the keyboard-reach follow-ups from that must-fix item.

| Target | Checklist | Pass | Fail | Pending | Unavailable |
| --- | --- | --- | --- | --- | --- |
| Apple Silicon macOS | [macos.md](macos.md) | 52 | 3 | 14 | 12 |

- **Build:** `main` at `d98fd1f` (0.1.2 plus Unreleased), `just package` on the
  reference Mac → `mdoc_0.1.2_arm64.dmg`, ad-hoc signed, not notarized
  (SHA-256 `13b35c3c…f150`). Installed over 0.1.2 for the run; the signed 0.1.2
  was put back afterwards.
- **Run by:** Claude through computer use from the Claude desktop app, reading
  the accessibility tree (`AXUIElement`) and the pasteboard. Clean profile
  (the app-data folder renamed and restored afterwards); the case key was not
  touched.
- **Machine:** the reference MacBook, Apple M4, 24 GB, macOS 15.7.7 (24G720),
  built-in Retina display only, U.S./ABC/Russian input sources.
- **Network:** about 30 KB/s during the run (a generic speed test agreed), with
  two drops. The 638 MB pseudonymization model never finished downloading, so
  the installed models were copied from the maintainer's profile for the
  popup, theme and clipboard rows.
- **Linux:** compiles in CI (`lint (ubuntu)`); no runtime checks.
- **Windows:** not part of this dry run. **Windows 10:** not checked.

## Fail rows

| Row | Failure | Triage | Link |
| --- | --- | --- | --- |
| R2 | When the next mention or proposal is already on screen, → or Enter scrolls the editor to the document's end and the popup is off-screen | Patch rule (wrong output, no design decision): issue in 0.1.3 | [Replacement popup scrolls the editor to the end when the next mention is already visible](https://github.com/lawkitt/mdoc/issues/102) |
| H18, A4 | Home's Recents rows are neither Tab stops nor in the accessibility tree (ADR 0062 says Tab from Home reaches Recents) | Not the patch rule; must-fix for 0.2.0 under the checklist's triage (a control unreachable by keyboard and screen reader), promoted at grooming | [Must-fix 2: Home's Recents rows](../../../../ROADMAP.md#must-fix) |
| H18, A4 | In the Keychain explainer, once Tab moves from Continue to Not now, Tab and ⇧Tab stay on Not now; Continue tracks a focus handle that is not a Tab stop (`workspace_keychain.rs`) | Not the patch rule; must-fix for 0.2.0 under the checklist's triage (unreachable by Tab inside a dialog), promoted at grooming | [Must-fix 3: Keychain explainer](../../../../ROADMAP.md#must-fix) |

H16 passes, but the run found
[⌘Q and Quit do nothing while a model downloads from its setup card](https://github.com/lawkitt/mdoc/issues/103)
(patch rule: an action fails silently), filed in 0.1.3.

## Unavailable rows

- **I1, I2:** local ad-hoc build; Gatekeeper and notarization need the CI
  `.dmg`.
- **D2:** no 1× external display at hand.
- **S2, S3, S4, S5:** the model downloads never finished at about 30 KB/s.
  Seen: byte progress, Cancel back to the card with Retry, the OCR consent
  card with its model warning, and a clean "Couldn't reach huggingface.co"
  error with Retry after a drop.
- **K3–K7:** no Japanese input source, and the maintainer does not run the
  Japanese rows (ADR 0057).

## Pending rows (the maintainer's)

- **Physical keyboard:** K1, K2, K8, K9; the ⇧ shortcuts H3 (⇧⌘I), H10 (⇧⌘H),
  H12 (⇧⌘P), and the ⇧ parts of passing rows: ⇧⌘S (H4, P2), ⇧⌘= (H7),
  ⇧⌘G and ⇧↩ (H11, injected and working), ⇧⌘X (H13), ⇧⌘Z (H14, R5), ⇧
  arrows and ⇧Tab (H15), ⌘⇧←/→ and ⌘⇧↑/↓ (H20), ⇧⌘⌫ (R3), ⌃⇧Tab (H6,
  injected and working), ⌥⇧⌘V (C3), ⌥⇧⌘C (C4).
- **System settings:** T3 (OS appearance), S1 (network off).
- **Browser:** C1 and H19's paste into the ChatGPT and Claude web apps (the
  pasteboard itself passes).
- **Keychain password and a saved case:** E2, E5, E3's Unlock… bar, P6's case
  creation, and "Categories to detect" (takes focus, holds Tab, returns focus
  on Esc or Close) with ⇧F10's case actions.
- **Spoken VoiceOver output:** A5, and A3's spoken titles.

## Automation notes

- Injected keys posted to mdoc's process reach GPUI (F6, Tab, arrows, ⇧F10),
  but not the out-of-process Open panel; the panel was pointed at the fixture
  folder through mdoc's own `NSOSPLastRootDirectory` default, restored
  afterwards.
- Window-only captures (`app_screenshot`) were used after full-screen captures
  twice showed the desktop while mdoc's window was hidden.
