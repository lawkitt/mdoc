# Windows keyboard reach smoke check

2026-10-22, Codex through T3 using `node_repl` and `@oai/sky`.
Windows 11 Pro, build 26200, Ryzen 9 3900X, 32 GB RAM, RTX 2070 SUPER.

This checks the implementation in PR [#89](https://github.com/lawkitt/mdoc/pull/89),
stacked on [#88](https://github.com/lawkitt/mdoc/pull/88) and
[#87](https://github.com/lawkitt/mdoc/pull/87). It is a debug build, with the
source changes committed alongside this note, not the installed RC acceptance
run required by [the checklist](../../../platform-acceptance.md).
Executable SHA-256:
`5de4e665d130e8c1ffcbdf523a038074c09c6af38d4a17c60fa7a1f525391780`.

The app used a temporary clean profile. The fixture was a public Markdown file,
`Public example.md`, in a folder opened as the case `Keyboard fixture`.
No maintainer documents or case keys were used. Screenshots were inspected for
Home and the public fixture; none were retained because no visual failure was
found. [UI Automation observations](windows-uia.json) record names, roles and
focus for the fixture's keyboard path.

| Check | Result | Observation |
| --- | --- | --- |
| Tab from Home | Pass | Focus moved from the window to Open files. Enter opened the native Open dialog; Esc dismissed it. |
| F6 on Home | Pass | Open files → Collapse sidebar → Open files, retaining Home's last control. Sidebar focus was visible. |
| F6 from the editor | Pass | Sidebar → toolbar; UI Automation reported Collapse sidebar and More actions respectively. |
| Sidebar Tab order | Pass | Collapse sidebar → Search → Home → Collapse case → active document row. |
| Shift+F10 on the active row | Pass | Opened the case menu; Tab reached Categories to detect. |
| Enter on Categories | Pass | Opened the category list; Tab reached its Close button. |
| F6 / Shift+F6 in Categories | Pass | Both kept focus on the list's Close button. |
| Esc from Categories | Pass | Closed it and returned focus to Public example.md's sidebar row. |
| More menu | Pass | Enter on More opened it; Tab reached Save; Esc returned to More. A second Esc returned to the editor. |

These use injected input. Physical F6 and Shift shortcuts, the collapsed rail's
native behavior, native macOS behavior, spoken Narrator/VoiceOver output, and
the full installed RC checklist remain to be checked. Headless regressions
cover the collapsed rail, narrow-window pane selection, opening a saved case
with Enter, and F6 staying in the macOS Keychain explainer.

## Automation and cleanup

| Tool / step | Result |
| --- | --- |
| `sky.list_windows`, input desktop probe | Available; `INPUT_DESKTOP=Default`. |
| `sky.launch_app` with the branch executable path | Selected the installed app instead; returned `launched app did not expose a targetable window: io.github.lawkitt.mdoc`. That instance was quit. The branch build was launched explicitly from PowerShell and identified by its returned executable path before any screenshot. |
| `sky.get_window_state`, `sky.press_key` | UI Automation and public-fixture screenshots worked. |
| State capture immediately after Ctrl+Q | `window is not a usable app window`; the process had exited. No input was retried. |
| Profile restoration | Original directory restored by rename; test profile retained in the temporary smoke directory. `PROFILE_RESTORED=True`, `BACKUP_EXISTS=False`, `MDOC_PROCESS_COUNT=0`; no mdoc windows remained. |

## Automated checks

- App: 351 passed, 12 ignored.
- Workspace excluding `gpui-pdf`: passed, including the editor, PII, spelling,
  integration and doc tests. An invalid code-fence comment found by rustdoc was
  corrected and its doc check rerun without warnings.
- `gpui-pdf` alone: 32 passed, 1 ignored. The full workspace invocation
  reproduced the four existing Windows scrolling-test failures listed under
  **[Tests] Flaky `gpui-pdf` scrolling tests** in [ROADMAP.md](../../../../ROADMAP.md).
- Workspace clippy with `-D warnings`, formatting and diff whitespace checks:
  passed.
