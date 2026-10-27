# Patch smoke run, 0.2.1: Apple Silicon macOS (2026-10-27)

Claude Code (desktop app) on the reference MacBook (Apple M4, macOS 15.8.1,
24H32), 2026-10-27, from a clean profile. The maintainer's profile was moved
aside first and restored afterwards; the run's profile is kept as
`~/Library/Application Support/mdoc.smoke-0.2.1-run`. The case key was not
touched.

Installed from the draft release's `mdoc_0.2.1_arm64.dmg` (SHA-256
`57d9e971fcfc52911997e8f402c160fe173e1161c9538fafc02a1ad8c57c30a0`, matches
`SHA256SUMS`), replacing 0.2.0 in `/Applications`.

No computer use this run: the maintainer declined it for mdoc. Checks were made
from the terminal and the accessibility tree's window titles; the window's
content tree read empty from outside (only the window and its traffic-light
buttons), so rows that need clicking or reading inside the window are
Pending (maintainer).

| ID | Check | Result | Notes |
| --- | --- | --- | --- |
| I1 | Install from the `.dmg` | Pass | Copied `mdoc.app` from the mounted image; `CFBundleShortVersionString` 0.2.1. |
| I2 | Notarization | Pass | `spctl -a -t exec`: accepted, `Notarized Developer ID`, Developer ID Application: Tebriz Tagiev (F33DLM8K53). |
| I5 | First launch | Pass | `open -a`: one window titled "mdoc" within 6 s; the process stayed up. Content not inspected (no computer use). |
| I6 | Version | Pass (bundle) | The bundle reports 0.2.1; Settings not opened. |
| P7 | Open With a scanned PDF (#166) | Pass | `open -a /Applications/mdoc.app russian.pdf` while running: the window became "russian.pdf — mdoc" (0.2.0 added no tab). Whether the OCR card shows is the maintainer's to see. |
| — | Open With a DOCX | Pass | `Settlement agreement.docx` opened as "Settlement agreement.md — mdoc". |
| — | Quit | Pass | AppleScript `quit`: the process exited within 1 s, no prompt (nothing edited). |
| C1 | Copy for AI | Pending (maintainer) | Needs Pseudonymize, Apply and the menu inside the window. |
| R7 | One ⌘Z undoes Apply all after an alias edit (#169) | Pending (maintainer) | |
| D5 | Rescan inside the window at 720 × 480 (#170) | Pending (maintainer) | |
| P4 | Sidebar tooltip after a Finder drop (#171) | Pending (maintainer) | |
| H18 | Tab past the toolbar shows a ring on every stop (#168) | Pending (maintainer) | |
| R2 | Enter applies after clicking a scope segment (#177) | Pending (maintainer) | |
| R3 | Undo inside the popup keeps it open (#178) | Pending (maintainer) | |

## Pending (maintainer)

C1, R7, D5, P4, H18, R2, R3 on the installed 0.2.1, plus a look at P7's OCR
card. Each fix has a headless regression test in CI.
