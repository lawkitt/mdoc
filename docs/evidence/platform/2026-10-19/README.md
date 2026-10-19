# Platform acceptance, 0.1.0-rc.1 (2026-10-19)

The native and platform acceptance run for 0.1.0
([ADR 0057](../../../adr/0057-native-platform-acceptance.md), template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)). In progress:
the macOS pass by Claude is done, the maintainer's macOS rows and the Windows
run are not.

| Target | Checklist | Pass | Fail | Pending | Unavailable |
| --- | --- | --- | --- | --- | --- |
| Apple Silicon macOS | [macos.md](macos.md) | 49 | 6 | 14 | 6 |
| Windows x64 | `windows.md` (maintainer) | | | | |

- **Build:** [v0.1.0-rc.1](https://github.com/lawkitt/mdoc/releases/tag/v0.1.0-rc.1),
  installed from `mdoc_0.1.0-rc.1_arm64.dmg` (SHA256SUMS verified, Notarized
  Developer ID) and `mdoc_0.1.0-rc.1_x64-setup.exe`.
- **macOS machine:** the reference MacBook, Apple M4, 24 GB, macOS 15.7.7
  (24G720), built-in Retina display only, U.S./ABC/Russian input sources.
- **Windows machine:** the maintainer's Windows 11 x64 PC (recorded in
  `windows.md`).
- **Linux:** compiles in CI (`lint (ubuntu)`); no runtime checks.
- **Windows 10:** not checked.

## Fail rows

| Row | Failure | Triage | Link |
| --- | --- | --- | --- |
| A2 | The popup's scope segments (This one, All N) and its category menu rows had no accessible name | Must-fix (unnamed controls) | Fixed after rc.1; rerun on rc.2 |
| A4, H18 | Without a screen reader, Tab cannot reach the toolbar or sidebar from Home or the editor | After 0.1.0: screen readers and Full Keyboard Access reach every control (ADR 0057, amended 2026-10-19) | [Item 14](../../../../ROADMAP.md) |
| A3 | Settings is not exposed as a dialog: no title announced, the document stays in the tree behind it | After 0.1.0 | [Item 14](../../../../ROADMAP.md) |
| P2 | Save As never adds the saved file to Recents | After 0.1.0 | [Item 14](../../../../ROADMAP.md) |
| P5 | An unsupported file shows the insertion line, not "No supported files" | After 0.1.0 | [Item 13](../../../../ROADMAP.md) |

## Unavailable rows (macOS)

- **D2:** no 1× external display at hand.
- **K3–K7:** no Japanese input source on the reference Mac, and the maintainer
  does not run the Japanese rows (ADR 0057). A headless test composes and
  commits Japanese in the preview find field.

## Pending rows (macOS, for the maintainer)

I1 (first-open prompt), T3 (live OS appearance switch), P6 and E2 (Continue
and the login password, then a saved case), E3 (Esc on the keyboard, the
Unlock… bar), E5, S1 (network off), C1 (paste into the ChatGPT and Claude web
apps), K1, K2, K8, K9 (keyboard layouts and the character palette), A5 (spoken
announcements), and H3 (Convert to Markdown in a source-only view).

## Observations

- Injected ⇧ shortcuts behave inconsistently: ⇧⌘S is ignored, while ⇧⌘P,
  ⇧⌘⌫ and ⌘⌥⇧C work. The maintainer confirmed ⇧⌘S, ⇧⌘H and ⌘↩ on a physical
  keyboard.
- Smaller findings without a row are in [After 0.1.0 item 14](../../../../ROADMAP.md):
  the missing ⌘-arrow caret moves, no Toggle Sidebar shortcut, mode 0600 on
  new files.
