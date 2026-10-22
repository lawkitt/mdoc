# Patch smoke run, 0.1.1 (2026-10-22)

The smoke run for the 0.1.1 patch ([ADR 0059](../../../adr/0059-roadmap-after-0-1-0.md)):
install, launch, open a document and Copy for AI on both platforms, plus the
rows the patch's fixes touch. Rows use the IDs of the
[checklist](../../../platform-acceptance.md).

## Before the release

| ID | Target | Check | Result | Notes |
| --- | --- | --- | --- | --- |
| T6 | Windows x64 | Caption buttons draw, work and snap layouts appear on hover | Pass | The maintainer, on the installed 0.1.0 with a physical mouse, 2026-10-22: minimize, maximize/restore and close act on a click, hovering maximize shows snap layouts, and a title-bar double-click toggles maximize and restore. Closes [#74](https://github.com/lawkitt/mdoc/issues/74); the 0.1.0 run's Fail is in [windows.md](../2026-10-19/windows.md). |

## On the 0.1.1 build

### Apple Silicon macOS

Codex by computer use (fleet ADR 0009) on the reference MacBook (Apple M4,
macOS 15.7.7, 24G720), 2026-10-22, from a clean profile; the original profile
was restored afterwards. Installed from `mdoc_0.1.1_arm64.dmg`
(SHA-256 `3e6c7356ae3d227b6bad1de000f1eb23f919d1905d29648fde574f5f2b6a638b`,
matches SHA256SUMS).

| ID | Check | Result | Notes |
| --- | --- | --- | --- |
| I1 | Install from the `.dmg` | Pass | Replaced `/Applications/mdoc.app`. |
| I2 | Notarization | Pass | `spctl`: accepted, `Notarized Developer ID`. |
| I5 | First launch | Pass | Home, no error or prompt. |
| I6 | Version | Pass | Settings shows 0.1.1. |
| C1 | Open a DOCX, Copy for AI | Pass | `text.docx`; with a replacement applied the clipboard holds the placeholder note and Markdown. Copy for AI is off until a replacement exists, as designed. |
| H13 | Bold with a trailing space (#77) | Pass | `**Plain** paragraph…`. |
| #75 | Save as case on an unsaved conversion | Pass | The Save dialog came first; `text.md` saved. The case write then waited on the Keychain password dialog, which computer use may not touch; the maintainer cancelled it, so "lists 1 document" is the maintainer's to confirm. |
| H16 | Mod+Q right after Save as case (#79) | Maintainer | Blocked by the same Keychain dialog. After it was cancelled, ⌘Q quit without clicking the document. |
| R5 | Popup undo closes it, redo still works (#79) | Pass | Applied count 1 → 0 (popup closed) → 1 with ⌘⇧Z. |
| P2 | Save As `Тест файл.md`, Recents (#76) | Pass | Listed in Home Recents; file mode `-rw-r--r--`. |
| — | ⌘⌥P Replace | Maintainer keyboard | Injected shortcut ignored; the Replace pill worked. |
| — | Quit exits the process (#83) | Pass | No `mdoc` process 10 s after ⌘Q, nor after the app menu's Quit. |

Automation notes: typing into the native Save dialog was unreliable (names
were checked in the AX tree before saving), and an AX query made after quit
relaunched mdoc.

### Windows x64

Not run yet: the dispatched session found the PC locked. Pending: install
(I3, I4), I5, I6, C1, H13, #75, H16, R5, P2, single-instance launch and Open
with (#80), and quit.
