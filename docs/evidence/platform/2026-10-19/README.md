# Platform acceptance, 0.1.0-rc.1 (2026-10-19)

The native and platform acceptance run for 0.1.0
([ADR 0057](../../../adr/0057-native-platform-acceptance.md), template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)). In progress:
Claude's macOS pass (rc.1) and Windows pass (rc.2, which reran A2) are done;
the maintainer's rows on both platforms are not.

| Target | Checklist | Pass | Fail | Pending | Unavailable |
| --- | --- | --- | --- | --- | --- |
| Apple Silicon macOS | [macos.md](macos.md) | 49 | 6 | 14 | 6 |
| Windows x64 | [windows.md](windows.md) | 47 | 4 | 17 | 5 |

- **Build:** [v0.1.0-rc.1](https://github.com/lawkitt/mdoc/releases/tag/v0.1.0-rc.1),
  installed from `mdoc_0.1.0-rc.1_arm64.dmg` (SHA256SUMS verified, Notarized
  Developer ID) on the Mac; [v0.1.0-rc.2](https://github.com/lawkitt/mdoc/releases/tag/v0.1.0-rc.2)
  from `mdoc_0.1.0-rc.2_x64-setup.exe` (SHA256SUMS verified) on Windows.
- **macOS machine:** the reference MacBook, Apple M4, 24 GB, macOS 15.7.7
  (24G720), built-in Retina display only, U.S./ABC/Russian input sources.
- **Windows machine:** the maintainer's PC, AMD Ryzen 9 3900X, 32 GB, RTX 2070
  SUPER, Windows 11 Pro 25H2 (26200.9550), one 1600 × 900 display at 100 %,
  English (US) and Russian input.
- **Linux:** compiles in CI (`lint (ubuntu)`); no runtime checks.
- **Windows 10:** not checked.

## Fail rows

| Row | Failure | Triage | Link |
| --- | --- | --- | --- |
| T6 (Windows) | Caption buttons do nothing when clicked and show no snap layouts; a title-bar double-click only maximizes | Should-fix | [0.1.0 stable: Windows caption buttons](../../../../ROADMAP.md) |
| A2 (macOS) | The popup's scope segments (This one, All N) and its category menu rows had no accessible name | Must-fix (unnamed controls) | Fixed after rc.1; passes on rc.2 (Windows A2) |
| A4 (macOS), H18 | Without a screen reader, Tab cannot reach the toolbar or sidebar from Home or the editor | After 0.1.0: screen readers and Full Keyboard Access reach every control (ADR 0057, amended 2026-10-19) | [RC acceptance follow-ups](../../../../ROADMAP.md) |
| A3 | Settings is not exposed as a dialog: no title announced, the document stays in the tree behind it (both platforms) | After 0.1.0 | [RC acceptance follow-ups](../../../../ROADMAP.md) |
| P2 | Save As never adds the saved file to Recents (both platforms) | After 0.1.0 | [RC acceptance follow-ups](../../../../ROADMAP.md) |
| P5 (macOS) | An unsupported file shows the insertion line, not "No supported files" | After 0.1.0 | [Dry-run polish](../../../../ROADMAP.md) |

## Unavailable rows (macOS)

- **D2:** no 1× external display at hand.
- **K3–K7:** no Japanese input source on the reference Mac, and the maintainer
  does not run the Japanese rows (ADR 0057). A headless test composes and
  commits Japanese in the preview find field.

## Unavailable rows (Windows)

- **K3–K7:** no Japanese input source, and the maintainer does not run the
  Japanese rows (ADR 0057).

## Pending rows (macOS, for the maintainer)

I1 (first-open prompt), T3 (live OS appearance switch), P6 and E2 (Continue
and the login password, then a saved case), E3 (Esc on the keyboard, the
Unlock… bar), E5, S1 (network off), C1 (paste into the ChatGPT and Claude web
apps), K1, K2, K8, K9 (keyboard layouts and the character palette), A5 (spoken
announcements), and H3 (Convert to Markdown in a source-only view).

## Pending rows (Windows, for the maintainer)

I3 (SmartScreen and the installer UI from a browser download), T3 (live OS
appearance switch), D3 and D4 (125 % and 150 % scale), P3–P6 (drops from File
Explorer, which Claude may only click), S1 (network off, models moved aside),
C1 (paste into the ChatGPT and Claude web apps), K1, K2, K9 (Russian layout,
Ctrl+Shift+;, AltGr or dead keys), R8 (Ctrl+\), R9 (Alt+Enter), A5 (Narrator
announcements), and H3 (Convert to Markdown in a source-only view).

## Observations

- Injected ⇧ shortcuts behave inconsistently: ⇧⌘S is ignored, while ⇧⌘P,
  ⇧⌘⌫ and ⌘⌥⇧C work. The maintainer confirmed ⇧⌘S, ⇧⌘H and ⌘↩ on a physical
  keyboard. On Windows injected Ctrl+Shift+S works, while Ctrl+\, Alt+Enter
  and Ctrl+Shift+; did nothing.
- Windows has no single-instance launch: each launch or "Open with" starts
  another process and window that restores the session.
- Smaller findings without a row are in [After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md):
  the missing ⌘-arrow caret moves and Windows Ctrl+arrow word moves, no
  Toggle Sidebar shortcut, mode 0600 on new files, palette gaps on Windows,
  and Settings focus not reported to UI Automation.
