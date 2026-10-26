# Platform acceptance, 0.2.0-rc.4 (2026-10-26)

The 0.2.0 acceptance run of the checklist
([ADR 0057](../../../adr/0057-native-platform-acceptance.md), template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)) on the CI
release candidate `v0.2.0-rc.4`, cut from `main` after the architecture
refactoring ([ADR 0069](../../../adr/0069-app-architecture-and-code-ownership.md))
(ROADMAP 0.2.0 must-fix 1), with the ship checks of must-fix 2. It includes
S1, A5, D3 and D4 deferred from 0.1.0, the Windows model downloads S2–S5 and
the keyboard-reach follow-ups. Run by [`docs/release-run.md`](../../../release-run.md).

| Target | Checklist | Pass | Fail | Pending | Unavailable |
| --- | --- | --- | --- | --- | --- |
| Apple Silicon macOS | [macos.md](macos.md) | 68 | 0 | 0 | 15 |
| Windows x64 | [windows.md](windows.md) | 55 | 2 | 0 | 24 |
| **Total** | | 123 | 2 | 0 | 39 |

- **Build:** GitHub pre-release `v0.2.0-rc.4`, tag commit `911dd0b`
  (`release: v0.2.0-rc.4`), built by CI. `mdoc_0.2.0-rc.4_arm64.dmg`, SHA-256
  `a705fc9d64f266bf8bbf4148d0dc0bb1eb70f61030560bcf482cd547eb79da54`,
  notarized Developer ID, team `F33DLM8K53`; `mdoc_0.2.0-rc.4_x64-setup.exe`,
  checked against `SHA256SUMS`, not Authenticode-signed.
- **macOS:** Claude through computer use from the Claude desktop app on the
  reference MacBook Air (Apple M4, 24 GB, macOS 15.7.7 24G720, built-in
  Retina only, U.S./ABC/Russian), from a clean profile, models copied from the
  maintainer's profile (no download on the Mac). The maintainer ran the
  keyboard, password, system-setting, browser and VoiceOver rows by hand the
  same day. The profile and the installed app were restored; the case key was
  not touched.
- **Windows:** Claude through computer use from the Claude desktop app on the
  maintainer's PC (Ryzen 9 3900X, 32 GB, RTX 2070 SUPER, Windows 11 Pro 26H2
  build 26300.9711, one 1600 × 900 display at 100 %), installed over 0.1.1
  from a renamed profile, with the real model downloads (S2–S5). The
  maintainer ran P6 and skipped the other maintainer rows for 0.2.0. The T3 Codex
  `windows-desktop` thread stopped at preflight ("Computer Use native pipe is
  unavailable"), so the maintainer started this run instead.
- **Linux:** compiles in CI (`lint (ubuntu)`); no runtime checks.
- **Windows 10:** not checked.

## Fail rows

| Row | Failure | Triage | Link |
| --- | --- | --- | --- |
| Windows H18, A4 | The popup's category menu and the panel's filter menu open but never take keyboard focus, so a category or filter can't be chosen by keyboard; ↓ in the open category menu steps the popup's mention instead | Must-fix (a keyboard dead end): rc.5. macOS was not checked for it, but the code is shared | [Popup category menu and panel filter menu don't take keyboard focus](https://github.com/lawkitt/mdoc/issues/167) |
| Windows H18 | In a wide window, Tab past the toolbar crosses seven controls that aren't drawn (the pane switch and the zoom pill) | Not must-fix: focus is hidden, not trapped; 0.2.1 | [Hidden pane switch and zoom controls are Tab stops](https://github.com/lawkitt/mdoc/issues/168) |

macOS P7 failed in the agent's run (a scanned PDF sent with `open -a` opened
nothing) but passed when the maintainer opened it from Finder › Open With; it
is recorded as Pass and stays open outside the milestone:
[Opening a scanned PDF from Finder (Open With) does nothing](https://github.com/lawkitt/mdoc/issues/166).
rc.1's D5 fail (#115) passes on rc.4.

## Unavailable rows

- **macOS D2:** no 1× external display.
- **macOS S2–S5:** no model download on the Mac (the run's choice); covered on
  Windows. S4's OCR card and S5's Installed state with copied models were seen.
- **Windows D2, X1, X5:** macOS-only rows.
- **K3–K7, both:** no Japanese input source; the maintainer does not run the
  Japanese rows (ADR 0057).
- **Skipped for 0.2.0 by the maintainer (2026-10-26):**
  - macOS: H3 (⇧⌘I), H10 (⇧⌘H), H12 (⇧⌘P) on the physical keyboard (each
    worked injected); H18's and the follow-up's ⇧F10 on a sidebar tab row (on
    a MacBook keyboard F10 is the mute key, so the check needs fn+⇧+F10).
  - Windows: I3 (SmartScreen), T3 (OS appearance), T6 (physical mouse), D3,
    D4 (display scale did not apply), P3–P5 (drags from Explorer), S1
    (network off), C1 (browser paste), K1, K2, K9, H15 (physical keyboard),
    A5 and X4 (Narrator speech).

The maintainer's own macOS checks (T3, P6, S1, C1, K1, K2, K8, K9, E2, E3's
Unlock…, E5, Categories, A3–A5 and the Recents names) passed; on Windows, P6
passed.

## Ship checks (ROADMAP 0.2.0 must-fix 2)

- **`just smoke-update` from 0.1.5:** cannot pass before 0.2.0 is published: a
  stable build reads only `/releases/latest`, which is 0.1.5 while rc.4 is a
  pre-release ("mdoc 0.1.5 is the latest version."). Run it again once 0.2.0
  is published (ROADMAP Engineering: an update check before publishing).
- **`just smoke-update` from rc.3 (stand-in):** the in-app update worked
  (`Updated mdoc 0.2.0-rc.3 to 0.2.0-rc.4.`), but the script's own
  `codesign -R` check had a quoting bug and had never passed; with it fixed
  ([#172](https://github.com/lawkitt/mdoc/pull/172)): `accepted`,
  `source=Notarized Developer ID`, `smoke-update passed`.
- **`scripts/keychain-launch-check.sh`:** not run. It needs a macOS account
  that never ran mdoc; on the maintainer's own account it stops ("a case key
  already exists"), as designed. Skipped for 0.2.0 by the maintainer (ROADMAP
  Engineering: run it against a throwaway keychain).

## Gate

#167 is must-fix, so the default gate applies: fix it, cut rc.5 as a draft,
and re-run the rows its fix touches on both platforms: H18, A4, R2 and R6,
plus #167's own steps (the popup's category menu, its "Belongs to…" owner
menu, and the panel's filter menu, by keyboard).

## Automation notes

- macOS: GPUI's accessibility tree appears after `AXManualAccessibility` is
  set; `AXFocused` was reported only for text fields and dialog buttons
  (rc.1's binary run the same way behaved the same), so focus was read from
  the drawn indicator in captures of mdoc's window (`screencapture -l`).
- macOS: injected System Events keystrokes can't type Cyrillic; the Open and
  Save panels' paths were set through the accessibility tree. GPUI dialog
  buttons were pressed through `AXPress`.
- macOS: drags from Finder need full-screen control; a press only starts a
  drag in a step after the item is selected.
