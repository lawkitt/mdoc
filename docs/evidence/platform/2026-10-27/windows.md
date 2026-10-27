# Windows x64, patch smoke run, 0.2.1 (2026-10-27)

The smoke run for the 0.2.1 patch on Windows: install, launch, open a document
and Copy for AI, plus the rows the patch's fixes touch. Rows use the IDs of the
[checklist](../../../platform-acceptance.md).

| Field | Value |
| --- | --- |
| Date | 2026-10-27 |
| Run by | Claude (Claude desktop app, computer use), started by the maintainer |
| mdoc version and artifact | 0.2.1, `mdoc_0.2.1_x64-setup.exe` from the v0.2.1 GitHub Release |
| SHA-256 | `46a9e1aaa84e392001d0d956dad30be72479c07cc1a35f7c1e8d076395fd7a99`, matches SHA256SUMS |
| Machine, CPU, RAM, GPU | The maintainer's PC: AMD Ryzen 9 3900X, 32 GB, NVIDIA GeForce RTX 2070 SUPER |
| OS and build | Windows 11 Pro 26H2, build 26300.9711 |
| Displays and scale | One display, 1600 × 900, 100 % |

The run started from a clean profile: `%LOCALAPPDATA%\mdoc` was renamed
before the install and restored afterwards, with `key.dpapi` unchanged. The
run's profile is kept as `%LOCALAPPDATA%\mdoc.smoke-0.2.1-run`. Because the
NSIS install lives in `%LOCALAPPDATA%\mdoc`, 0.2.1 was installed again into
the restored profile, as in the 0.1.1 run.

| ID | Check | Result | Notes |
| --- | --- | --- | --- |
| I3 | NSIS installation | Pass | Silent install from PowerShell (`/S`): exit code 0, per-user into `C:\Users\tebri\AppData\Local\mdoc`, uninstall entry 0.2.1. VC++ runtime already present (14.51.36247). The installer was not run from Explorer, so SmartScreen was not exercised. |
| I4 | Start menu and uninstaller | Pass | `Start Menu\Programs\mdoc\mdoc.lnk` points to the installed `mdoc.exe`, and `uninstall.exe` is present and registered. Launching the shortcut started 0.2.1. The 0.2.0 MSI install in `C:\Program Files\mdoc` is still present, with its own all-users shortcut. |
| I5 | First launch | Pass | Home on the clean profile, with no error or prompt. Launched from a script, the window opened minimized. |
| I6 | Version | Pass | Settings shows "You have mdoc 0.2.1." |
| C1 | Open a DOCX, Pseudonymize, Copy for AI | Pass | `Settlement agreement.docx`: Pseudonymize proposed 14 entities after the Standard model downloaded (Settings, about 632 MB). One replacement was applied (`Mia` → `PERSON_3`). The clipboard held the placeholder note, then the Markdown with `PERSON_3`. |
| H18, A4 (#168) | Maximized 1600 × 900, F6 to the toolbar, Tab onward: visible focus, Tab reaches the editor | Fail | The view switch (`Edit`, `Original`, `Pseudonymized`, `Side by side`) and the zoom pill (`Zoom out`, `Reset zoom`, `Zoom in`) appear when focused and show focus. `More actions` and `Apply 23` show focus, and Tab from `Zoom in` enters the editor. But `Hide replacements` and `Hide the original file` show no focus indicator while their panels are open: captures of them focused and unfocused are pixel-identical. Focus was read from UI Automation. [#194](https://github.com/lawkitt/mdoc/issues/194), milestone 0.2.2, not must-fix. |
| R7 (#169) | Edit the popup's Alias field + Enter, Ctrl+Enter, one Ctrl+Z | Pass | Clicked `Harrow & Finch Ltd`, then set the alias to `ACME_CO` with Enter. Ctrl+Enter applied all 24 replacements. One Ctrl+Z, with no click into the editor, went back to "23 still to replace". |
| D5 (#170) | Replacements open, window at its minimum size | Pass | Client area 720 × 480 (window 736 × 488); "↻ Rescan" is fully inside the panel footer and the window. |
| P4 (#171) | Drop two `.md` files from File Explorer between two sidebar tabs; the tooltip names the new tab | Pending (maintainer) | File Explorer was granted click-only to computer use (no drag), so this was not run. |
| R2 (#177) | Click a scope segment, then Enter | Pass | "This one" + Enter applied only that mention (22 left) and opened 2/4 with "This one" still selected. "All 4" + Enter applied the remaining 3 (19 left) and opened the next proposal (`Employer`). |
| R3 (#178) | Ctrl+Backspace in the popup, Ctrl+Z, Ctrl+Backspace | Pass | The keep marked `Employer` kept (3 kept). Ctrl+Z restored it with the popup still open. The second Ctrl+Backspace kept it again in the popup, and the text was unchanged. |
| P7 (#166) | With mdoc running, Explorer "Open with" → mdoc on `russian.pdf` | Pending (maintainer) | File Explorer was click-only (no right-click), so this was not run. Supporting check: passing the path to the installed `mdoc.exe` while mdoc was running, as Open with does, opened a `russian.pdf` tab with the OCR card ("This page needs text recognition", English & Russian suggested), in the same process. |
| — | Clean quit | Pass | Ctrl+Q showed the save prompt for the unsaved conversion. After Discard, no `mdoc` process remained at the first check, well within 10 s. |

## Pending (maintainer)

- **P4 (#171):** drop two `.md` files from File Explorer between two sidebar
  tabs, keep the pointer still over the first new row, and check that its
  tooltip names the new tab.
- **P7 (#166):** with mdoc running, use File Explorer → Open with → mdoc on
  `tests/fixtures/ocr-qualification/russian.pdf`, and check that a tab opens
  with the OCR card.

## Notes

- The v0.2.1 GitHub Release was already published, not a draft, when the run
  downloaded its assets.
- Automation: the computer-use grant for "mdoc" first resolved to the 0.2.0
  MSI's `C:\Program Files\mdoc\mdoc.exe`, so the installed 0.2.1 needed a
  separate grant for its own `mdoc.exe`.
