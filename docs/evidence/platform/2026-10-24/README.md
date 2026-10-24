# Platform acceptance, 0.2.0-rc.1 (2026-10-24)

The 0.2.0 acceptance run of the checklist
([ADR 0057](../../../adr/0057-native-platform-acceptance.md), template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)) on the CI
release candidate `v0.2.0-rc.1` (ROADMAP 0.2.0 must-fix 1). It includes S1 and
A5, deferred from 0.1.0, and the keyboard-reach follow-ups from that must-fix
item. Layout follows the [2026-10-23 dry run](../2026-10-23-dry-run/README.md).

| Target | Checklist | Pass | Fail | Pending | Unavailable |
| --- | --- | --- | --- | --- | --- |
| Apple Silicon macOS | [macos.md](macos.md) | 58 | 1 | 14 | 10 |
| Windows x64 | `windows.md` (run by the T3 session; not pushed yet) | – | – | – | – |
| **Total** | | 58 | 1 | 14 | 10 |

- **Build:** draft GitHub Release `v0.2.0-rc.1`, tag commit `c3dc4ff`
  (`release: v0.2.0-rc.1`), built by CI. macOS artifact
  `mdoc_0.2.0-rc.1_arm64.dmg`, SHA-256
  `0c70cf2cb1dcfc18289a2ea9678062f245a29c71e1f91aa06ce580b2ac7da709`
  (matches the release's `SHA256SUMS`); notarized Developer ID, team
  `F33DLM8K53`.
- **Run by (macOS):** Claude through computer use from the Claude desktop app,
  reading the accessibility tree (`AXUIElement`, including `AXFocused`) and
  the pasteboard. Clean profile (the app-data folder renamed and restored
  afterwards); the case key was not touched. The RC replaced the installed
  0.1.4 for the run; 0.1.4 was put back afterwards.
- **Machine (macOS):** the reference MacBook Air, Apple M4, 24 GB, macOS 15.7.7
  (24G720), built-in Retina display only, U.S./ABC/Russian input sources.
- **Models (macOS):** the network was slow, so no model was downloaded on the
  Mac. The setup cards (S1, S4) were checked on the clean profile; then the
  installed models (Standard pseudonymization, English OCR) were **copied from
  the maintainer's profile** for the popup, theme and clipboard rows.
  Real-download rows (S2–S5) are covered by the Windows run.
- **Linux:** compiles in CI (`lint (ubuntu)`); no runtime checks.
- **Windows 10:** not checked.

## Fail rows

| Row | Failure | Triage | Link |
| --- | --- | --- | --- |
| D5 | At the minimum window size, with Find and the Replacements panel open, the find field is 0 pt wide and Next match / Close search are covered by the panel; the keyboard still reaches them | Not must-fix (no data loss, no flow blocked, reachable by keyboard and screen reader); issue in 0.2.0 | [Find bar controls hidden under the Replacements panel at the minimum window size](https://github.com/lawkitt/mdoc/issues/115) |

The two dry-run fails pass on the RC: Home's Recents rows are named and
reachable by Tab (Must-fix 2), and Tab / ⇧Tab cycle the Keychain explainer's
two buttons (Must-fix 3). The dry run's #102 (popup scrolls to the end) also
passes in R2.

## Unavailable rows

- **D2:** no 1× external display at hand.
- **S2, S3, S4, S5:** no model download on the Mac (slow network, by
  agreement); covered by the Windows run. S4's OCR consent card and S5's
  Installed state (with copied models) were seen.
- **K3–K7:** no Japanese input source, and the maintainer does not run the
  Japanese rows (ADR 0057).

## Pending rows (the maintainer's)

- **Physical keyboard:** K1, K2, K8, K9; the ⇧ shortcuts H3 (⇧⌘I), H10 (⇧⌘H,
  injected and working), H12 (⇧⌘P, injected and working), and the ⇧ parts of
  passing rows: ⇧⌘S (H4, P2; injected did nothing), ⇧⌘= (H7), ⇧⌘G and ⇧↩
  (H11), ⇧⌘X (H13), ⇧⌘Z (H14, R5), ⇧ arrows and ⇧Tab (H15), ⌘⇧←/→ and
  ⌘⇧↑/↓ (H20), ⇧⌘⌫ (R3), ⌃⇧Tab (H6), ⌥⇧⌘V (C3), ⌥⇧⌘C (C4); the injected ones
  worked unless noted.
- **System settings:** T3 (OS appearance), S1 (network off).
- **Browser:** C1 and H19's paste into the ChatGPT and Claude web apps (the
  pasteboard itself passes).
- **Keychain password and a saved case:** E2, E5, E3's Unlock… bar, P6's case
  creation, ⇧F10's case actions on a saved case, and "Categories to detect"
  (takes focus, holds Tab, returns focus on Esc or Close).
- **Spoken VoiceOver output:** A5, A3's spoken titles, and the Home Recents
  row names (named in the accessibility tree).

## Automation notes

- `gh release download` sets no quarantine attribute, so a browser-style
  `com.apple.quarantine` was added to the `.dmg` to see the first-launch
  Gatekeeper prompt (I1).
- Injected keys reached mdoc and, this time, the Open and Save panels
  (Go to Folder). Injected ⇧⌘S did nothing, as in the dry run.
- `AXFocused` is exposed for GPUI controls, so focus moves (F6, Tab, the
  explainer) were read from the tree; the editor itself reports no focused
  element.
- Reading a menu item through System Events without opening the menu showed
  File › Copy for AI as disabled and pressing it did nothing; a real click on
  the open menu worked (menus validate on open).
