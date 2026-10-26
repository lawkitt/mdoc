# Windows platform acceptance, 0.2.0-rc.4

The Windows half of the 2026-10-26 run, from the template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)
([ADR 0057](../../../adr/0057-native-platform-acceptance.md)), with the
0.2.0 must-fix rows from [ROADMAP.md](../../../../ROADMAP.md) (S1, A5, D3, D4,
the model downloads S2–S5 and the keyboard-reach follow-ups in section 12).
Claude ran it from the Claude desktop app on the maintainer's PC through
computer use, reading state from UI Automation (`System.Windows.Automation`:
names, roles, toggle and selection states, and the focused element after each
key) and the clipboard. Rows that need Narrator speech, a physical keyboard,
passwords or system settings are **Pending (maintainer)**. Shortcuts were
injected without a keyboard layout: an ignored shortcut is not a failure on
its own. File Explorer was granted for plain clicks only, so no drag started
from it. Fixtures were copied to `%TEMP%\mdoc-acceptance\Документы\Папка с пробелами\`.

## Header

| Field | Value |
| --- | --- |
| Date | 2026-10-26 |
| Run by | Claude (computer use, UI Automation, clipboard reads); the maintainer changed system settings on request |
| mdoc version and artifact | 0.2.0-rc.4, `mdoc_0.2.0-rc.4_x64-setup.exe` (SHA256SUMS verified; GitHub pre-release; not Authenticode-signed), installed silently (`/S`) over 0.1.1 after renaming the profile |
| Machine, CPU, RAM, GPU | The maintainer's PC, AMD Ryzen 9 3900X, 32 GB, NVIDIA GeForce RTX 2070 SUPER |
| OS and build | Windows 11 Pro 26H2, build 26300.9711 |
| Displays and scale | One display, 1600 × 900 at 100 % (96 DPI) |
| Input sources installed | English (US), Russian (no Japanese) |

## 1. Install and first launch

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| I3 | *Windows*: SmartScreen shows, "Run anyway" proceeds (unsigned betas), the installer completes, and the VC++ Redistributable is installed or already present | Pending (maintainer) | The installer completed silently (exit 0) and the Apps entry reads 0.2.0-rc.4; VC++ 14.51 was already present. SmartScreen needs a browser download with Mark of the Web, and "Run anyway" is a system prompt. |
| I4 | *Windows*: Start menu entry and uninstaller are present; the app launches from Start | Pass | Start › mdoc › `mdoc.lnk` and `uninstall.exe`; Apps lists "mdoc 0.2.0-rc.4" (publisher lawkitt). Launched from its Start entry. The uninstaller was not run. |
| I5 | First launch shows Home with no error, no crash report notice, and no Keychain or network prompt | Pass | Clean profile: Home in Dark (System), no error, crash notice or prompt. |
| I6 | About/Settings shows the expected version | Pass | Settings › Updates: "mdoc 0.2.0-rc.4 is the latest version." |

## 2. Themes

Fixture: the English demo DOCX, with the Source preview open and aliases applied.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| T1 | Light: shell, sidebar, editor, Replacements panel, popup, dialogs and menus are legible, with no unthemed or invisible controls | Pass | Shell, sidebar, editor, panel (Review and Share), popup, its category menu, the panel filter menu, palette, Settings, notices strip and the unsaved-changes dialog legible. |
| T2 | Dark: the same | Pass | The same in Dark (the first half of the run); preview pages follow the theme. |
| T3 | System: switching the OS appearance while mdoc is open updates every visible surface without a restart | Pending (maintainer) | mdoc was set to Theme: System and the maintainer was asked to switch Windows to Light, but `AppsUseLightTheme` stayed 0 and mdoc's own theme button changed instead, so the OS switch was not exercised. |
| T4 | Original page tone follows the theme; the override in the Original header works in both themes | Pass | Dark: themed pages; "Show original paper" switches to white pages and back. Light: pages are already white. |
| T5 | Highlights (proposed, applied, search, spelling) stay distinguishable in both themes | Pass | Proposed (category tints), applied (blue), search (yellow, current orange) and spelling (red) are distinct in both themes. |
| T6 | *Windows*: caption buttons draw, work and snap layouts appear on hover | Pending (maintainer) | The buttons draw and highlight on hover; an injected click on Maximize did nothing, as in the 0.1.0 run, where the maintainer's physical mouse worked. A title-bar double-click maximizes and restores. Needs the physical mouse. |

## 3. Display scaling

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| D2 | *macOS*: a 1× external display; text is legible and the window moves between displays without layout breakage (Unavailable without such a monitor) | Unavailable | macOS-only row; this PC has no macOS and no 1× display (ROADMAP 0.2.0 must-fix 1). |
| D3 | *Windows*: 125 % scale; text crisp, hit targets aligned with what is drawn | Pending (maintainer) | The maintainer set 125 % during the run, but the monitor and mdoc's window still reported 96 DPI (custom scaling usually applies only after signing out), so 125 % was never on screen. |
| D4 | *Windows*: 150 % scale; the same, including the popup and dialogs | Pending (maintainer) | As D3. |
| D5 | Resizing to the minimum window size keeps every control reachable | Pass | The minimum is 736 × 488 px; the header, Markdown/Source switch, Pseudonymize and Apply stay visible. An injected drag on the window border did not resize; the size was set through `SetWindowPos`, which honours the minimum. |

## 4. Files and paths

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| P1 | Open (Mod+O) `Соглашение об урегулировании.docx` from `Документы/Папка с пробелами/`; it converts and its preview shows | Pass | Ctrl+O with the path typed into the dialog; converted, Source preview shown, RU spelling underline on «Маришей». |
| P2 | Save As (Mod+Shift+S) into the same folder under a Cyrillic name with spaces; reopening it from Recents works | Pass | `Мировое соглашение копия` (typed without extension) was saved as `…копия.md`; it appears in Recents ("…in Папка с пробелами, opened just now") and reopens from there after Ctrl+W. Save As for an Untitled tab starts in `%LOCALAPPDATA%\mdoc`, the app's own folder. |
| P3 | Dropping that file onto Home opens it | Pending (maintainer) | File Explorer is click-only for computer use; no drag can start from it. |
| P4 | Dropping two files onto the sidebar shows the insertion line and opens both at that position | Pending (maintainer) | As P3. |
| P5 | Dragging an unsupported file over Home or the sidebar says "These files can't be opened" (no insertion line) and opens nothing | Pending (maintainer) | As P3. |
| P6 | Dropping a folder opens it as a case | Pending (maintainer) | As P3. Blocking maintainer row (ROADMAP 0.2.0). |
| P7 | Opening a file from the OS (Finder/Explorer "Open with") opens it in mdoc | Pass | `mdoc.exe "<path>"` (what "Open with" runs) opened `reference.pdf` with its preview, and the scanned `russian.pdf` with its OCR card, each as a new tab in the running window (one process; 0.1.0 opened a second one). The macOS failure #166 does not reproduce on Windows. |

## 5. First-use model setup

Run on the clean profile.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| S1 | Offline (network off): Pseudonymize shows the consent card; Download & continue fails cleanly with Retry and no crash | Pending (maintainer) | The maintainer chose to skip turning the network off. Needs a profile without the pseudonymization model. |
| S2 | Online: Retry downloads with byte progress, then Verifying / Checking runtime, and the scan resumes in the requesting tab | Pass | After S3's cancel, Retry downloaded with byte progress (67 → 683 MB), showed "Checking runtime…", and the scan resumed in the requesting Russian tab (14 identities, Apply 18). Verifying passed too fast to see. |
| S3 | Cancel during download returns to the card; a second attempt completes | Pass | Download & scan, Cancel at 51 / 709 MB: "Download cancelled" with Retry (focused), Cancel and Choose model…, "683 MB left to download"; Retry (Enter) resumed and completed. |
| S4 | Converting `reference.pdf` with OCR shows the OCR consent card and completes after the download | Pass | `reference.pdf` has a text layer, so the scanned `tests/fixtures/ocr-qualification/russian.pdf` was used: the card preselected English & Russian ("the file name mentions Russian"), Download & recognize fetched 22 MB and produced the text exactly, ё included. |
| S5 | Settings shows both models as installed after setup | Pass | Settings shows English & Russian OCR and Standard pseudonymization as Installed (their Download buttons gone). "Installed" is not part of the radio buttons' UI Automation names. |

## 6. Replacement popup and panel

Fixture: the English demo, scanned (14 identities, Apply 24).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| R1 | Clicking a proposal opens the popup next to it, inside the window, with buttons inside the popup | Pass | The popup opens below the mention, inside the window, every control inside it. |
| R2 | Enter applies in the shown segment (All N, then This one) without changing it and opens the next proposal; ⌫ keeps, ←/→ step through mentions, Mod+↑/↓ jump to the ends, Esc closes | Pass | All 6: Enter applied six Ms Bennett mentions (24 → 18) and opened Mia. This one: after → (2/3, segment kept) Enter applied one Employer mention (13 → 12) and moved to 3/3 with This one kept. ⌫ kept Mia; → 3/4, Ctrl+↓ 4/4, Ctrl+↑ 1/4; Esc closes. Right after clicking a segment, focus is on that segment button, so Enter presses it instead of applying. |
| R3 | Mod+⌫ keeps the same text; Mod+Shift+⌫ keeps the whole entity | Pass | Ctrl+⌫ and Ctrl+Shift+⌫ each kept the three Employer mentions ("Kept 3"); Ctrl+Z undid each. Undo closes the popup, so the next popup shortcut needs the popup reopened. |
| R4 | Editing the alias field and pressing Enter renames the alias | Pass | ORG_2 → EMPLOYER with Enter; the panel follows. |
| R5 | Mod+Z inside the popup and the panel undoes the last decision; Mod+Shift+Z (and Ctrl+Y on Windows) redoes | Pass | In the popup Ctrl+Z undid a keep, Ctrl+Y and Ctrl+Shift+Z redid it; Ctrl+Z works in the panel (R6). |
| R6 | Panel list: ↑/↓, Space, Enter, →/←, ⌫ behave as described in ADR 0048 | Pass | From the panel search ↓ enters the list at the last active row with a visible ring; Space and ⌫ keep, ← / → collapse and expand, Ctrl+Z undoes, Enter reveals the mention. UI Automation reports the window, not the row, as focused while arrowing through the list (Narrator follow-up in A5). |
| R7 | Mod+Enter and the toolbar's Apply N apply all and close the popup; one Mod+Z undoes them | Pass | Ctrl+Enter applied all (23 replaced, 1 kept), closed the popup and switched to Share; one Ctrl+Z restored 13 proposals. The toolbar's Apply 13 did the same. |
| R8 | Before / After (Mod+\\) shows both columns; Esc leaves | Pass | Injected Ctrl+\ opened Original and Pseudonymized columns; Esc left. |
| R9 | Alt+Enter, Alt+↑/↓ and Mod+Alt+P work in the editor | Pass | Alt+↓ / Alt+↑ and Alt+Enter step to a candidate and open its popup; Ctrl+Alt+P on a selected "laptop" added laptop → REDACTED_1 with Cancel (cancelled). |

## 7. Clipboard

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| C1 | Copy for AI, pasted into the ChatGPT and Claude web apps, arrives as plain text with the placeholder note | Pending (maintainer) | Browsers are not allowed for computer use. |
| C2 | Copy Markdown of the Russian demo, pasted into TextEdit (plain text mode) or Notepad, is byte-identical to the saved file, Cyrillic intact; the same for a line with CJK text | Pass | Copy Markdown of the Russian demo is byte-identical to its file (3,844 bytes, LF); `cjk.md` (契約 テスト 合意 — 한국어 文字, 52 bytes) is byte-identical too. Compared from the clipboard, not through Notepad. |
| C3 | Paste AI Response (Mod+Alt+Shift+V) of a browser selection carrying HTML and plain text uses the plain text and restores aliases | Pass | A clipboard with HTML (bold, red span, an HTML-only word) and plain text, pasted with Ctrl+Alt+Shift+V, opened "Restored – Settlement agreement.md": "Summary. Maya Bennett may settle with Harrow & Finch Ltd; Ms Bennett keeps the reference." |
| C4 | Pseudonymize Clipboard (Mod+Alt+Shift+C) replaces the known case names in the clipboard; pasting elsewhere shows the aliases | Pass | "Maya Bennett emailed Harrow & Finch Ltd…; Ms Bennett keeps…" became "PERSON_1 emailed ORG_1…; PERSON_2 keeps…", with the notice "Replaced 3 names from this document in the clipboard." |
| C5 | Copy, cut and paste between two editor tabs keep the text exactly | Pass | Ctrl+A Ctrl+C in the Russian tab, Ctrl+V into a new tab, Ctrl+A Ctrl+X, Ctrl+V: the clipboard and the new tab's Markdown are byte-identical to the file each time. |

## 8. Keyboard input and IME

Install the Russian layout and the built-in Japanese IME first. The
maintainer does not run the Japanese rows (K3–K7): mark them Unavailable
unless Claude observed them on macOS (ADR 0057).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| K1 | Russian typed into the editor and into the find field appears correctly | Pending (maintainer) | Computer use enters non-ASCII text through the clipboard, not the Russian layout. Pasted Russian in the editor and find field displays and finds correctly. |
| K2 | A misspelled Russian word gets the spelling underline; Mod+; and Mod+Shift+; move between misspellings | Pending (maintainer) | The underline shows on «Маришей» and «Шахова»; injected Ctrl+; selected nothing (layout-dependent key). |
| K3 | Japanese in the editor: the marked text is underlined while composing | Unavailable | No Japanese input source; the maintainer does not run the Japanese rows (ADR 0057). |
| K4 | The candidate window opens at the caret, not at the window corner | Unavailable | As K3. |
| K5 | Choosing a candidate commits it; Esc during composition cancels it | Unavailable | As K3. |
| K6 | One undo removes the committed Japanese text | Unavailable | As K3. |
| K7 | Japanese in the find field composes, commits and finds the text | Unavailable | As K3. |
| K9 | Dead keys or Option/AltGr characters (e.g. `é`, `€`) type correctly | Pending (maintainer) | Needs a physical keyboard. |

## 9. Shortcuts

Every application shortcut, from `bind_app_keys`, `bind_markdown_search_keys`,
`bind_shell_keys`, `pii::ui::bind_keys` and `mdoc_editor::bind_keys`.

| ID | Shortcut | Action | Result | Note |
| --- | --- | --- | --- | --- |
| H1 | Mod+N | New | Pass | New tab opens. |
| H2 | Mod+O | Open | Pass | Open dialog (P1). |
| H3 | Mod+Shift+I | Convert to Markdown | Pass | `counts.csv` (with a Cyrillic row) opened source-only with the Convert card; Ctrl+Shift+I converted it to a Markdown table, Cyrillic intact. |
| H4 | Mod+S / Mod+Shift+S | Save / Save As | Pass | Ctrl+S wrote an edit to `Мировое соглашение копия.md`; Ctrl+Shift+S opens Save As (P2). |
| H5 | Mod+W | Close tab | Pass | Closes the tab. |
| H6 | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab | Pass | Ctrl+Tab wraps from the last tab to the first; Ctrl+Shift+Tab returns. |
| H7 | Mod+= , Mod+Shift+= , Mod+- , Mod+0 | Zoom in, in, out, reset | Pass | Ctrl+= ×3 → 115 %, Ctrl+Shift+= → 120 %, Ctrl+- → 115 %, Ctrl+0 → 100 %. |
| H8 | Mod+, | Settings; Esc closes, Tab/Shift+Tab move between fields | Pass | Ctrl+, opens Settings; Tab and Shift+Tab move with a visible ring and are now reported to UI Automation; Esc closes. |
| H9 | Mod+K | Palette; ↑/↓, Enter, Esc | Pass | ↓ ↓ ↑ highlight Open files…; Enter runs (Copy Markdown, Copy for AI, Toggle sidebar); Esc closes. The highlighted row is not exposed as selected to UI Automation. |
| H10 | Mod+Shift+H | Home | Pass | Shows Home. |
| H11 | Mod+F, Mod+G, Mod+Shift+G, Enter, Shift+Enter, Esc | Find, next, previous, close | Pass | «Павлов» 7/7, Ctrl+G 1/7, Enter 2/7, Shift+Enter 1/7, Ctrl+Shift+G 7/7; Esc closes. |
| H12 | Mod+Shift+P | Pseudonymize | Pass | Scanned the English demo (14 identities, Apply 24). |
| H13 | Mod+B, Mod+I, Mod+E, Mod+U, Mod+Shift+X | Bold, italic, code, underline, strike | Pass | Copy Markdown: `*alpha* **beta** <u>gamma</u> ~~delta~~ `` `epsilon` ``. |
| H14 | Mod+A, Mod+C, Mod+X, Mod+V, Mod+Z, Mod+Shift+Z (Ctrl+Y on Windows) | Editing | Pass | Exercised in C5 and R5. |
| H15 | Alt+←/→, Alt+Shift+←/→, Home/End, Shift+arrows, Tab/Shift+Tab | Word moves, selection, indent | Pending (maintainer) | Alt+←, Alt+Shift+→, Home, End, Shift+← work; Tab indents a list item (`    - second`) and Shift+Tab outdents it. Injected Shift+End (and Shift+Home without Ctrl) selected nothing: physical keyboard. |
| H16 | Mod+Q | Quit, with the unsaved-changes prompt | Pass | Ctrl+Q shows an in-app dialog "Save changes to "russian.md"?" (Save focused, Discard, Cancel) per dirty tab; Discard on each (six) quits. |
| H17 | *Windows*: each File-menu action is reachable from the window | Pass | The palette lists New tab, Open files…, Paste text as new tab, Pseudonymize, Copy for AI, Copy Markdown, Save, Save as…, Before / After, Close tab, Restore AI reply, Pseudonymize clipboard, Open a case…, theme, Toggle sidebar, Home and Settings; Convert to Markdown is in the source-only view (H3). |
| H18 | F6 / Shift+F6 cycle sidebar, toolbar, editor, Replacements, Original and Comments (open ones only), returning to the last control in each; Tab / Shift+Tab move between controls with a visible focus ring and, at a region's last (first) control, on to the next (previous) region, so Tab past the toolbar enters the editor; in the sidebar ↑/↓ move between tabs, ⌫ closes one and Shift+F10 opens its case actions; the category list keeps focus until closed; Esc closes menus and returns from the sidebar or toolbar to the editor (ADR 0062) | Fail | F6 editor → Replacements → Original → sidebar → toolbar → editor, landing on each region's last control; Shift+F6 reverses; Esc from the toolbar and sidebar returns to the editor; sidebar ↑/↓, ⌫ and Shift+F10 work (section 12); Tab stays inside the popup, Settings and dialogs. Fails: the popup's category menu and the panel's filter menu never take focus, so they cannot be used by keyboard ([#167](https://github.com/lawkitt/mdoc/issues/167), must-fix); and Tab past the toolbar crosses seven hidden stops (pane switch, zoom pill) with no visible ring before entering the editor ([#168](https://github.com/lawkitt/mdoc/issues/168)). |
| H19 | Enter, Shift+Enter in the editor | New paragraph, line break; Copy for AI pasted into a chat keeps both apart (ADR 0060) | Pass | Copy for AI gives `two Xthree four Z\n\npara2\\\nline2`; the `\` hides away from the caret. Pasting into a chat is part of C1. |
| H22 | *Windows*: Ctrl+←/→, Ctrl+Home/End, each with Shift; Ctrl+Backspace, Ctrl+Delete | Word moves, document start / end, selection, word deletion | Pass | From "one two three four five": Ctrl+Backspace removed "five", Ctrl+Home then Ctrl+Delete removed "one ", Ctrl+→ moved to the start of "three", Ctrl+End, Ctrl+Shift+Home selected all, Ctrl+Shift+← selected words (H13). |

## 10. Case key storage

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| E1 | Nothing asks for the key at launch | Pass | No prompt at first launch or after quit and relaunch. |
| E4 | *Windows*: saving a case asks nothing (DPAPI) | Pass | Sidebar Shift+F10 › Save mapping as case asked only to save the unsaved conversion (`Settlement case.md`), then saved at once; `cases\key.dpapi` and the `.case` file appeared. |
| E5 | After quit and relaunch, opening a case member restores its replacements without a prompt | Pass | After Ctrl+Q and relaunch the session restored the member with its aliases; closed and reopened from Home › Cases › Settlement case, it showed every alias with no prompt. |

## 11. Screen reader basics

VoiceOver (⌘F5) on macOS, Narrator (Ctrl+Win+Enter) on Windows. The editor's
text is not exposed to assistive technology (known limitation, ADR 0057); do
not mark it Fail.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| A1 | Sidebar toggle, tabs, toolbar buttons and the Pseudonymize control announce a name and role | Pass | UI Automation: Collapse sidebar, Search or run an action, Home, the "Open documents" tab list with named tab items (with a DOCX / MD suffix for same-named tabs), New Markdown tab, Settings, Theme, Help, Hide replacements, Hide the original file, More actions, Pseudonymize, Apply N and Share are named. Each tab's close button now names its tab ("Close russian.md"). Spoken output not checked (A5). |
| A2 | Replacements panel controls and popup buttons announce a name and role | Pass | Popup: "Replace · untick to keep 3 originals" checkbox, Alias edit, "Correct category for selected scope", This one / All N toggles, Previous / Next mention, "Close replacement popup"; category rows are named toggles. Panel rows read e.g. "Ms Bennett to PERSON_2, 6 mentions, 0 applied, 0 kept, expanded"; setup and OCR cards' buttons are named. |
| A3 | Settings and the Keychain/unsaved-changes dialogs announce their title and buttons; focus stays inside | Pass | Settings is now a window named "Settings" with focus inside; Tab and Shift+Tab wrap (Done ↔ English) and F6 does nothing. The unsaved-changes dialog is a window "Save changes to "russian.md"?" with Save (focused), Discard, Cancel; Tab wraps. |
| A4 | Every control in A1–A3 can be reached and activated by keyboard alone (F6, Tab, arrows, Enter, Space), from Home and from the editor, without a screen reader or Full Keyboard Access (ADR 0062) | Fail | Home: Tab reaches Open files…, the action buttons, the Cases row and each Recents row with a visible ring. Regions, sidebar, panel list, popup controls, Settings and dialogs are reachable (H18). The popup's category menu and the panel's filter menu open but their rows cannot be reached ([#167](https://github.com/lawkitt/mdoc/issues/167)). |
| A5 | Scan status and toasts are announced | Pending (maintainer) | Needs Narrator speech. Status texts ("Download cancelled", "Checking runtime…", the clipboard notice) are not in the UI Automation tree as text; the maintainer should also listen for focus changes in the panel list and palette, where UI Automation reports no focused row. Blocking maintainer row (ROADMAP 0.2.0). |

## 12. Keyboard-reach follow-ups (ROADMAP 0.2.0, from #89)

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| X1 | F6 stays in the Keychain explainer | Unavailable | macOS-only: Windows saves cases through DPAPI with no explainer (E4). |
| X2 | Shift+F10 opens a sidebar tab's case actions in both sidebar layouts | Pass | Expanded sidebar and collapsed rail (F6 opens its reveal on the remembered tab): Shift+F10 on the case member opened Categories to detect…, Rename case…, Remove from case, Delete case and forget originals…, Close tab; Tab reaches them. On a tab outside a case it offers Save mapping as case and Close tab. |
| X3 | Categories to detect takes focus, holds Tab and returns focus on Esc or Close | Pass | The list takes focus; Tab cycles Close and the eight categories and wraps, F6 does nothing; Esc and Close each return focus to the "Settlement case.md" sidebar row. |
| X4 | Narrator reads each Home Recents row's name | Pending (maintainer) | Speech not checked. Each row is a Tab stop with role Button and a full name, e.g. "Мировое соглашение копия.md in Папка с пробелами, opened just now" and "Settlement agreement.docx in Папка с пробелами, pseudonymized, opened 7 min ago". |
| X5 | Tab and Shift+Tab cycle the Keychain explainer's two buttons | Unavailable | macOS-only, as X1. |

## Summary

- **81 rows: Pass 54, Fail 2, Unavailable 8, Pending (maintainer) 17.**
- **Fail:** H18 and A4 — [#167](https://github.com/lawkitt/mdoc/issues/167)
  (popup category menu and panel filter menu don't take keyboard focus;
  must-fix, a keyboard dead end) and, for H18 only,
  [#168](https://github.com/lawkitt/mdoc/issues/168) (hidden pane switch and
  zoom controls are Tab stops; 0.2.1).
- **Unavailable:** D2, X1, X5 (macOS-only); K3–K7 (no Japanese input source,
  ADR 0057).
- **Pending (maintainer):** I3 (SmartScreen), T3 (OS appearance), T6
  (physical mouse), D3, D4 (display scale did not apply), P3–P6 (drags from
  Explorer; P6 blocking), S1 (network off), C1 (browser paste), K1, K2, K9,
  H15 (physical keyboard), A5 and X4 (Narrator speech; A5 blocking).
- Windows 10: not checked.
