# Platform acceptance, 0.1.0-rc.1 (2026-10-19)

The native and platform acceptance run for 0.1.0
([ADR 0057](../../../adr/0057-native-platform-acceptance.md), template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)). macOS is
complete: Claude's pass on rc.1 with A2 and H3 rerun on rc.2, and the
maintainer's rows; S1 and A5 were not run for 0.1.0. Windows is
complete: Claude's pass on rc.2 (which reran A2), H3 on rc.2, T6 retested on
rc.3 and on a local build of the fix, and the maintainer's rows; D3, D4, S1
and A5 were not run for 0.1.0.

| Target | Checklist | Pass | Fail | Pending | Unavailable |
| --- | --- | --- | --- | --- | --- |
| Apple Silicon macOS | [macos.md](macos.md) | 62 | 5 | 0 | 8 |
| Windows x64 | [windows.md](windows.md) | 60 | 4 | 0 | 9 |

- **Build:** [v0.1.0-rc.1](https://github.com/lawkitt/mdoc/releases/tag/v0.1.0-rc.1),
  installed from `mdoc_0.1.0-rc.1_arm64.dmg` (SHA256SUMS verified, Notarized
  Developer ID) on the Mac, with A2 and H3 rerun on `mdoc_0.1.0-rc.2_arm64.dmg`;
  [v0.1.0-rc.2](https://github.com/lawkitt/mdoc/releases/tag/v0.1.0-rc.2) from `mdoc_0.1.0-rc.2_x64-setup.exe` (SHA256SUMS verified) on Windows,
  with T6 retested on `mdoc_0.1.0-rc.3_x64-setup.exe` (the release run's
  artifact) and on a local release build of `dcfb904`.
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
| T6 (Windows) | Caption buttons do nothing when clicked and show no snap layouts (rc.2, rc.3); a title-bar double-click only maximizes (rc.2) | Should-fix; fixed in `dcfb904` (physical clicks work on a local build), ships in 0.1.0 | [0.1.0 stable: Windows caption buttons](../../../../ROADMAP.md) |
| A4 (macOS), H18 | Without a screen reader, Tab cannot reach the toolbar or sidebar from Home or the editor | After 0.1.0: screen readers and Full Keyboard Access reach every control (ADR 0057, amended 2026-10-19) | [RC acceptance follow-ups](../../../../ROADMAP.md) |
| A3 | Settings is not exposed as a dialog: no title announced, the document stays in the tree behind it (both platforms) | After 0.1.0 | [RC acceptance follow-ups](../../../../ROADMAP.md) |
| P2 | Save As never adds the saved file to Recents (both platforms) | After 0.1.0 | [RC acceptance follow-ups](../../../../ROADMAP.md) |
| P5 (macOS) | An unsupported file shows the insertion line, not "No supported files" | After 0.1.0 | [Dry-run polish](../../../../ROADMAP.md) |

A2 (macOS) failed on rc.1: the popup's scope segments and category menu rows
had no accessible name (must-fix). Fixed after rc.1; passes on rc.2 on both
platforms.

## Unavailable rows (macOS)

- **D2:** no 1× external display at hand.
- **S1, A5:** not run for 0.1.0: the maintainer deferred the offline setup
  check and spoken VoiceOver output (2026-10-19;
  [After 0.1.0: Deferred acceptance rows](../../../../ROADMAP.md)).
- **K3–K7:** no Japanese input source on the reference Mac, and the maintainer
  does not run the Japanese rows (ADR 0057). A headless test composes and
  commits Japanese in the preview find field.

## Unavailable rows (Windows)

- **D3, D4, S1, A5:** not run for 0.1.0: the maintainer deferred display
  scaling, the offline setup check and spoken Narrator output (2026-10-19;
  [After 0.1.0: Deferred acceptance rows](../../../../ROADMAP.md)).
- **K3–K7:** no Japanese input source, and the maintainer does not run the
  Japanese rows (ADR 0057).

## Observations

- Injected ⇧ shortcuts behave inconsistently: ⇧⌘S is ignored, while ⇧⌘P,
  ⇧⌘⌫ and ⌘⌥⇧C work. The maintainer confirmed ⇧⌘S, ⇧⌘H and ⌘↩ on a physical
  keyboard. On Windows injected Ctrl+Shift+S works, while Ctrl+\, Alt+Enter
  and Ctrl+Shift+; did nothing; the maintainer confirmed all three on a
  physical keyboard (R8, R9, K2). Injected hover and clicks never reach the
  Windows caption buttons, even where a physical mouse works (T6).
- Windows has no single-instance launch: each launch or "Open with" starts
  another process and window that restores the session.
- Smaller findings without a row are in [After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md):
  the missing ⌘-arrow caret moves and Windows Ctrl+arrow word moves, no
  Toggle Sidebar shortcut, mode 0600 on new files, palette gaps on Windows,
  and Settings focus not reported to UI Automation.
