# Platform acceptance checklist

The manual checklist for each shipped target before a stable release
([ADR 0057](adr/0057-native-platform-acceptance.md)). Headless tests and the
release gates do not establish what it checks.

## Running it

1. Install the release candidate from its CI artifacts: the `.dmg` on the
   Apple Silicon Mac, the NSIS `.exe` on the Windows x64 PC.
2. Start from a clean profile: quit mdoc and rename its app-data folder
   (`dirs::data_local_dir()/mdoc`: `~/Library/Application Support/mdoc`,
   `%LOCALAPPDATA%\mdoc`) to restore it afterwards. Never delete the case key
   (`io.github.lawkitt.mdoc` / `case-key`): saved cases become unreadable. A
   first Keychain prompt (E2) needs a macOS user account that never ran mdoc;
   without one, mark E2 Unavailable if no prompt appears. The explainer
   contains a captioned drawing of the macOS dialog; the real one is a
   separate SecurityAgent window that appears only after Continue. In a
   screenshot-driven run, check for that window (for example with
   `CGWindowListCopyWindowInfo`) rather than judging by the picture.
   Rows that need a physical keyboard (section 8, shortcuts with ⇧), spoken
   VoiceOver output, or the Keychain password are run by hand, not by
   computer use.
3. Copy this file to `docs/evidence/platform/<date>/macos.md` or `windows.md`
   and fill in the header and every row. Rows marked *macOS* or *Windows* apply
   only to that platform; on the other one, delete them.
4. Mark each row **Pass**, **Fail** or **Unavailable**. Fail and Unavailable
   need a note. A Fail row links the fix or the roadmap item it became.
   Screenshots go only with failures, and show public fixtures only.
5. `README.md` in the same folder gives the summary: build, machines, counts,
   every Unavailable row with its reason, every Fail row with its link,
   "Linux: compiles in CI (`lint (ubuntu)`); no runtime checks", and
   "Windows 10: not checked".

### Running it with an agent

An agent runs both checklists through computer use (ADR 0057). Dispatched
runs use Codex through T3 (fleet ADR 0009), proven on both machines; Claude
can also run them from the Claude desktop app, started by the maintainer.
Before the first screenshot, only public fixtures may be on screen: start from
the clean profile in step 2, since mdoc restores the last session's documents.
On Windows `%LOCALAPPDATA%\mdoc` also holds the installed `mdoc.exe`, so the
renamed folder still contains the program; launch it from there or reinstall. A session starting cold reads this file,
ADR 0057 and the latest `docs/evidence/platform/<date>/README.md`.

- **Read state from the accessibility tree**, not only screenshots: on macOS
  `AXUIElement` (a small Swift script, as in the 2026-10-19 run), on Windows UI
  Automation (`System.Windows.Automation` from PowerShell, or Accessibility
  Insights). Names, roles, toggled and selected states there are what
  VoiceOver and Narrator read.
- **Injected keys carry no keyboard layout.** A shortcut that injection
  ignores is not a Fail on its own; mark it for the maintainer's physical
  keyboard. Typed text goes through the platform text input and is fine for
  Cyrillic.
- **System settings are the maintainer's:** SmartScreen "Run anyway" (I3),
  display scaling (D3, D4), the network for offline setup (S1), the OS
  appearance (T3) and adding input sources. Ask, wait, then check the result.
- **Never delete the case key.** On Windows it is `key.dpapi` inside
  `%LOCALAPPDATA%\mdoc\cases…`, so renaming the app-data folder for a clean
  profile moves it too; restore the folder at the end. On macOS it is the
  Keychain item above.
- **Only public fixtures** on screen and in notes; close or avoid the
  maintainer's own windows and files.
- **Finish** by committing `<platform>.md`, updating the counts, Fail,
  Unavailable and Pending lists in the run's `README.md`, and linking every
  Fail row to a fix or a roadmap item (by its title, not its number).

Fixtures: the demos in [`docs/product-media/`](product-media/)
(`Settlement agreement.md`/`.docx`, `Соглашение об урегулировании.md`/`.docx`),
`tests/fixtures/docx-preview/legal-opinion.docx` and `tests/fixtures/reference.pdf`.
Copy a demo to a folder named `Документы/Папка с пробелами/` for the path rows.
"Mod" is ⌘ on macOS and Ctrl on Windows.

Failure triage: a failure that loses data, blocks a listed flow, or leaves a
control unreachable by screen reader (or by Tab inside panels, the popup and
dialogs) is must-fix; fix it, cut a
new RC and rerun the affected rows. Anything else becomes an After 0.1.0
roadmap item.

---

## Header

| Field | Value |
| --- | --- |
| Date | |
| Run by | |
| mdoc version and artifact | |
| Machine, CPU, RAM, GPU | |
| OS and build | |
| Displays and scale | |
| Input sources installed | |

## 1. Install and first launch

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| I1 | *macOS*: the `.dmg` opens; dragging to Applications and launching shows no Gatekeeper warning beyond the standard "downloaded from the Internet" prompt | | |
| I2 | *macOS*: `spctl -a -vv /Applications/mdoc.app` reports `Notarized Developer ID` | | |
| I3 | *Windows*: SmartScreen shows, "Run anyway" proceeds (unsigned betas), the installer completes, and the VC++ Redistributable is installed or already present | | |
| I4 | *Windows*: Start menu entry and uninstaller are present; the app launches from Start | | |
| I5 | First launch shows Home with no error, no crash report notice, and no Keychain or network prompt | | |
| I6 | About/Settings shows the expected version | | |

## 2. Themes

Fixture: the English demo DOCX, with the Source preview open and aliases applied.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| T1 | Light: shell, sidebar, editor, Replacements panel, popup, dialogs and menus are legible, with no unthemed or invisible controls | | |
| T2 | Dark: the same | | |
| T3 | System: switching the OS appearance while mdoc is open updates every visible surface without a restart | | |
| T4 | Original page tone follows the theme; the override in the Original header works in both themes | | |
| T5 | Highlights (proposed, applied, search, spelling) stay distinguishable in both themes | | |
| T6 | *macOS*: traffic lights sit on the unified title row; *Windows*: caption buttons draw, work and snap layouts appear on hover | | |

## 3. Display scaling

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| D1 | *macOS*: Retina display; text and icons are crisp | | |
| D2 | *macOS*: a 1× external display; text is legible and the window moves between displays without layout breakage (Unavailable without such a monitor) | | |
| D3 | *Windows*: 125 % scale; text crisp, hit targets aligned with what is drawn | | |
| D4 | *Windows*: 150 % scale; the same, including the popup and dialogs | | |
| D5 | Resizing to the minimum window size keeps every control reachable | | |

## 4. Files and paths

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| P1 | Open (Mod+O) `Соглашение об урегулировании.docx` from `Документы/Папка с пробелами/`; it converts and its preview shows | | |
| P2 | Save As (Mod+Shift+S) into the same folder under a Cyrillic name with spaces; reopening it from Recents works | | |
| P3 | Dropping that file onto Home opens it | | |
| P4 | Dropping two files onto the sidebar shows the insertion line and opens both at that position | | |
| P5 | Dragging an unsupported file over Home or the sidebar says "These files can't be opened" (no insertion line) and opens nothing | | |
| P6 | Dropping a folder opens it as a case | | |
| P7 | Opening a file from the OS (Finder/Explorer "Open with") opens it in mdoc | | |

## 5. First-use model setup

Run on the clean profile.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| S1 | Offline (network off): Pseudonymize shows the consent card; Download & continue fails cleanly with Retry and no crash | | |
| S2 | Online: Retry downloads with byte progress, then Verifying / Checking runtime, and the scan resumes in the requesting tab | | |
| S3 | Cancel during download returns to the card; a second attempt completes | | |
| S4 | Converting `reference.pdf` with OCR shows the OCR consent card and completes after the download | | |
| S5 | Settings shows both models as installed after setup | | |

## 6. Replacement popup and panel

Fixture: the English demo, scanned.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| R1 | Clicking a proposal opens the popup next to it, inside the window, with buttons inside the popup | | |
| R2 | Enter applies in the shown segment (All N, then This one) without changing it and opens the next proposal; ⌫ keeps, ←/→ step through mentions, Mod+↑/↓ jump to the ends, Esc closes | | |
| R3 | Mod+⌫ keeps the same text; Mod+Shift+⌫ keeps the whole entity | | |
| R4 | Editing the alias field and pressing Enter renames the alias | | |
| R5 | Mod+Z inside the popup and the panel undoes the last decision; Mod+Shift+Z (and Ctrl+Y on Windows) redoes | | |
| R6 | Panel list: ↑/↓, Space, Enter, →/←, ⌫ behave as described in ADR 0048 | | |
| R7 | Mod+Enter and the toolbar's Apply N apply all and close the popup; one Mod+Z undoes them | | |
| R8 | Before / After (Mod+\\) shows both columns; Esc leaves | | |
| R9 | Alt+Enter, Alt+↑/↓ and Mod+Alt+P work in the editor | | |

## 7. Clipboard

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| C1 | Copy for AI, pasted into the ChatGPT and Claude web apps, arrives as plain text with the placeholder note | | |
| C2 | Copy Markdown of the Russian demo, pasted into TextEdit (plain text mode) or Notepad, is byte-identical to the saved file, Cyrillic intact; the same for a line with CJK text | | |
| C3 | Paste AI Response (Mod+Alt+Shift+V) of a browser selection carrying HTML and plain text uses the plain text and restores aliases | | |
| C4 | Pseudonymize Clipboard (Mod+Alt+Shift+C) replaces the known case names in the clipboard; pasting elsewhere shows the aliases | | |
| C5 | Copy, cut and paste between two editor tabs keep the text exactly | | |

## 8. Keyboard input and IME

Install the Russian layout and the built-in Japanese IME first. The
maintainer does not run the Japanese rows (K3–K7): mark them Unavailable
unless Claude observed them on macOS (ADR 0057).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| K1 | Russian typed into the editor and into the find field appears correctly | | |
| K2 | A misspelled Russian word gets the spelling underline; Mod+; and Mod+Shift+; move between misspellings | | |
| K3 | Japanese in the editor: the marked text is underlined while composing | | |
| K4 | The candidate window opens at the caret, not at the window corner | | |
| K5 | Choosing a candidate commits it; Esc during composition cancels it | | |
| K6 | One undo removes the committed Japanese text | | |
| K7 | Japanese in the find field composes, commits and finds the text | | |
| K8 | *macOS*: Ctrl+⌘+Space opens the character palette and inserts an emoji | | |
| K9 | Dead keys or Option/AltGr characters (e.g. `é`, `€`) type correctly | | |

## 9. Shortcuts

Every application shortcut, from `bind_app_keys`, `bind_markdown_search_keys`,
`bind_shell_keys`, `pii::ui::bind_keys` and `mdoc_editor::bind_keys`.

| ID | Shortcut | Action | Result | Note |
| --- | --- | --- | --- | --- |
| H1 | Mod+N | New | | |
| H2 | Mod+O | Open | | |
| H3 | Mod+Shift+I | Convert to Markdown | | |
| H4 | Mod+S / Mod+Shift+S | Save / Save As | | |
| H5 | Mod+W | Close tab | | |
| H6 | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab | | |
| H7 | Mod+= , Mod+Shift+= , Mod+- , Mod+0 | Zoom in, in, out, reset | | |
| H8 | Mod+, | Settings; Esc closes, Tab/Shift+Tab move between fields | | |
| H9 | Mod+K | Palette; ↑/↓, Enter, Esc | | |
| H10 | Mod+Shift+H | Home | | |
| H11 | Mod+F, Mod+G, Mod+Shift+G, Enter, Shift+Enter, Esc | Find, next, previous, close | | |
| H12 | Mod+Shift+P | Pseudonymize | | |
| H13 | Mod+B, Mod+I, Mod+E, Mod+U, Mod+Shift+X | Bold, italic, code, underline, strike | | |
| H14 | Mod+A, Mod+C, Mod+X, Mod+V, Mod+Z, Mod+Shift+Z (Ctrl+Y on Windows) | Editing | | |
| H15 | Alt+←/→, Alt+Shift+←/→, Home/End, Shift+arrows, Tab/Shift+Tab | Word moves, selection, indent | | |
| H16 | Mod+Q | Quit, with the unsaved-changes prompt; with two or more edited or pseudonymized tabs, the summary (Review…, Discard All, Cancel) first (ADR 0071) | | |
| H17 | *macOS*: every item in the mdoc, File, Edit, View and Window menus runs its action and shows its shortcut; Edit › Paste works in the find field and in a Save dialog (ADR 0058). *Windows*: each File-menu action is reachable from the window | | |
| H18 | F6 / Shift+F6 cycle sidebar, toolbar, editor, Replacements, Original and Comments (open ones only), returning to the last control in each; Tab / Shift+Tab move between controls with a visible focus ring and, at a region's last (first) control, on to the next (previous) region, so Tab past the toolbar enters the editor; in the sidebar ↑/↓ move between tabs, ⌫ closes one and Shift+F10 opens its case actions; the category list keeps focus until closed; Esc closes menus and returns from the sidebar or toolbar to the editor (ADR 0062) | | |
| H19 | Enter, Shift+Enter in the editor | New paragraph, line break; Copy for AI pasted into a chat keeps both apart (ADR 0060) | | |
| H20 | *macOS*: ⌘←/→ on a wrapped paragraph and a list item, ⌘↑/↓, each with ⇧ | Visual line start / end (after a list marker first), document start / end; ⇧ selects (ADR 0061) | | |
| H21 | *macOS*: ⌥⌫, ⌥⌦, ⌘⌫ | Delete word left, word right, to the line start | | |
| H22 | *Windows*: Ctrl+←/→, Ctrl+Home/End, each with Shift; Ctrl+Backspace, Ctrl+Delete | Word moves, document start / end, selection, word deletion | | |

## 10. Case key storage

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| E1 | Nothing asks for the key at launch | | |
| E2 | *macOS*: Save mapping as case shows the "mdoc will ask for Keychain access" explainer, then the system dialog; Always Allow saves the case | | |
| E3 | *macOS*: Not now (Esc) cancels the action, and the locked-cases bar offers Unlock… | | |
| E4 | *Windows*: saving a case asks nothing (DPAPI) | | |
| E5 | After quit and relaunch, opening a case member restores its replacements without a prompt | | |
| E6 | Mod+W on an edited, pseudonymized tab without a case asks once, "Save “…” and keep its mapping?"; Save as Case saves the file and the case and closes the tab. Mod+Q with a saved, pseudonymized tab without a case asks "Keep the replacement mapping?" (ADR 0071) | | |

## 11. Screen reader basics

VoiceOver (⌘F5) on macOS, Narrator (Ctrl+Win+Enter) on Windows. The editor's
text is not exposed to assistive technology (known limitation, ADR 0057); do
not mark it Fail.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| A1 | Sidebar toggle, tabs, toolbar buttons and the Pseudonymize control announce a name and role | | |
| A2 | Replacements panel controls and popup buttons announce a name and role | | |
| A3 | Settings and the Keychain/unsaved-changes dialogs announce their title and buttons; focus stays inside | | |
| A4 | Every control in A1–A3 can be reached and activated by keyboard alone (F6, Tab, arrows, Enter, Space), from Home and from the editor, without a screen reader or Full Keyboard Access (ADR 0062) | | |
| A5 | Scan status and toasts are announced | | |
