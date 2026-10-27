# Patch smoke run, 0.2.1 (2026-10-27)

The smoke run for the 0.2.1 patch ([ADR 0059](../../../adr/0059-roadmap-after-0-1-0.md)):
install, launch, open a document and Copy for AI on both platforms, plus the
rows the patch's fixes touch. Rows use the IDs of the
[checklist](../../../platform-acceptance.md).

Build: tag `v0.2.1` (release commit `4fab496`); the Release workflow passed CI,
packaging and the installed Windows desktop smoke. The maintainer published the
release on 2026-10-27 at 12:33 UTC, while the Windows run was in progress.

| Platform | Runner | Pass | Fail | Pending (maintainer) | Evidence |
| --- | --- | --- | --- | --- | --- |
| Apple Silicon macOS | Claude Code, terminal and window titles only (computer use declined for mdoc) | 7 | 0 | 7 | [macos.md](macos.md) |
| Windows x64 | Claude desktop app by computer use (the T3 Codex desktop pipe was unavailable again) | 9 | 1 | 2 | [windows.md](windows.md) |

## Fixes checked

| Issue | Row | macOS | Windows |
| --- | --- | --- | --- |
| #166 Open With a scanned PDF | P7 | Pass (`open -a`; OCR card not seen) | Pending (supporting check passed: path to the running app opened the OCR card) |
| #168 hidden Tab stops | H18, A4 | Pending | Fail on a different control: [#194](https://github.com/lawkitt/mdoc/issues/194). The view switch and zoom pill now show focus, as fixed. |
| #169 Undo after Apply all | R7 | Pending | Pass |
| #170 Rescan at minimum size | D5 | Pending | Pass |
| #171 stale sidebar tooltip | P4 | Pending | Pending |
| #177 Enter after a scope segment | R2 | Pending | Pass |
| #178 Undo inside the popup | R3 | Pending | Pass |

## Fail

- **Windows H18/A4:** the toolbar's `Hide replacements` and `Hide the original
  file` toggles show no focus ring while their panels are open
  ([#194](https://github.com/lawkitt/mdoc/issues/194), milestone 0.2.2, not
  must-fix). This is the "toolbar toggle's on fill hides it" item in the
  roadmap's Later list.

## Pending (maintainer)

- **macOS:** C1, R7, D5, P4, H18, R2, R3 on the installed 0.2.1, and a look at
  P7's OCR card.
- **Windows:** P4 (a File Explorer drop) and P7 (File Explorer › Open with).
