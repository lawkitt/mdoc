# Windows platform acceptance, 0.2.0-rc.4

The Windows half of the 2026-10-26 run, from the template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)
([ADR 0057](../../../adr/0057-native-platform-acceptance.md)), with the
0.2.0 must-fix rows from [ROADMAP.md](../../../../ROADMAP.md) (S1, A5, D3, D4,
the model downloads S2–S5 and the keyboard-reach follow-ups in section 12).
Claude ran it from the Claude desktop app on the maintainer's PC through
computer use, reading state from UI Automation (`System.Windows.Automation`)
and the clipboard. Rows that need Narrator speech, a physical keyboard,
passwords or system settings are **Pending (maintainer)**. Shortcuts were
injected without a keyboard layout: an ignored shortcut is not a failure on
its own. File Explorer was granted for plain clicks only, so no drag started
from it.

*Run in progress.*

## Header

| Field | Value |
| --- | --- |
| Date | 2026-10-26 |
| Run by | Claude (computer use, UI Automation, clipboard reads) |
| mdoc version and artifact | 0.2.0-rc.4, `mdoc_0.2.0-rc.4_x64-setup.exe` (SHA256SUMS verified; GitHub pre-release; not Authenticode-signed), installed silently (`/S`) over 0.1.1 after renaming the profile |
| Machine, CPU, RAM, GPU | The maintainer's PC, AMD Ryzen 9 3900X, 32 GB, NVIDIA GeForce RTX 2070 SUPER |
| OS and build | Windows 11 Pro 26H2, build 26300.9711 |
| Displays and scale | One display, 1600 × 900 at 100 % |
| Input sources installed | |

## 1. Install and first launch

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| I3 | *Windows*: SmartScreen shows, "Run anyway" proceeds (unsigned betas), the installer completes, and the VC++ Redistributable is installed or already present | Pending (maintainer) | The installer completed silently (exit 0) and the Apps entry reads 0.2.0-rc.4; VC++ 14.51 was already present. SmartScreen needs a browser download with Mark of the Web and "Run anyway" is a system prompt: maintainer. |
| I4 | *Windows*: Start menu entry and uninstaller are present; the app launches from Start | Pass | Start › mdoc › `mdoc.lnk` and `uninstall.exe`; Apps lists "mdoc 0.2.0-rc.4" (publisher lawkitt). Launched from its Start entry. The uninstaller was not run. |
| I5 | First launch shows Home with no error, no crash report notice, and no Keychain or network prompt | Pass | Clean profile: Home in Dark (System), no error, crash notice or prompt. |
| I6 | About/Settings shows the expected version | Pass | Settings › Updates: "mdoc 0.2.0-rc.4 is the latest version." |
