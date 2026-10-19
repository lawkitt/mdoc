# Windows platform acceptance, 0.1.0-rc.2

The Windows half of the [2026-10-19 run](README.md), from the template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)
([ADR 0057](../../../adr/0057-native-platform-acceptance.md)). Claude ran it
from Claude Code on the maintainer's PC through computer use, reading state
from UI Automation (`System.Windows.Automation`) and the clipboard; rows it
could not observe were run by the maintainer the same day (notes start
"Maintainer:"); rows still Pending are marked *Maintainer*. Shortcuts were
injected without a keyboard layout: an ignored shortcut is not a failure on
its own.
File Explorer was granted for plain clicks only, so no drag started from it.

## Header

| Field | Value |
| --- | --- |
| Date | 2026-10-19 |
| Run by | Claude (computer use, UI Automation, clipboard reads); the maintainer ran the rows whose notes start "Maintainer:" |
| mdoc version and artifact | 0.1.0-rc.2, `mdoc_0.1.0-rc.2_x64-setup.exe` (SHA256SUMS verified; draft release; not Authenticode-signed), installed silently over 0.1.0-beta.4 |
| Machine, CPU, RAM, GPU | The maintainer's PC, AMD Ryzen 9 3900X, 32 GB, NVIDIA GeForce RTX 2070 SUPER |
| OS and build | Windows 11 Pro 25H2, build 26200.9550 |
| Displays and scale | One AOC 2001 display, 1600 × 900 at 100 % |
| Input sources installed | English (US), Russian (no Japanese) |

## 1. Install and first launch

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| I3 | *Windows*: SmartScreen shows, "Run anyway" proceeds (unsigned betas), the installer completes, and the VC++ Redistributable is installed or already present | Pass | Maintainer: downloaded in a browser; SmartScreen showed, Run anyway proceeded and the installer UI completed. VC++ 14.51 was already present. |
| I4 | *Windows*: Start menu entry and uninstaller are present; the app launches from Start | Pass | Apps list "mdoc 0.1.0-rc.2" (publisher lawkitt) with `uninstall.exe`; Start › mdoc launches it. The uninstaller was not run; its NSIS template deletes only the app's files and a then-empty folder, so models, cases and `key.dpapi` in the same folder stay. |
| I5 | First launch shows Home with no error, no crash report notice, and no Keychain or network prompt | Pass | Clean profile: Home in Dark (System), no error, crash notice or prompt. |
| I6 | About/Settings shows the expected version | Pass | Settings › Updates: "You have mdoc 0.1.0-rc.2." |

## 2. Themes

Fixture: the English demo DOCX, with the Source preview open and aliases applied.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| T1 | Light: shell, sidebar, editor, Replacements panel, popup, dialogs and menus are legible, with no unthemed or invisible controls | Pass | Shell, sidebar, editor, panel (Review and Share), popup, More actions menu, palette and Settings legible. The toolbar's "…" glyph is faint. |
| T2 | Dark: the same | Pass | The same in Dark; the preview pages follow the theme. |
| T3 | System: switching the OS appearance while mdoc is open updates every visible surface without a restart | Pass | Maintainer: switching the Windows appearance while mdoc was open updated every visible surface without a restart. |
| T4 | Original page tone follows the theme; the override in the Original header works in both themes | Pass | Dark: "Show original paper" switches to white pages and back. Light hides the control because the pages are already original. |
| T5 | Highlights (proposed, applied, search, spelling) stay distinguishable in both themes | Pass | Proposed (category tints), applied (blue), search (yellow, current orange) and spelling (red) are distinct in both themes. A match that wraps also paints a block at the end of the previous line ([After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md)). |
| T6 | *Windows*: caption buttons draw, work and snap layouts appear on hover | Fail | The buttons draw and highlight on hover, but clicking minimize, maximize/restore or close does nothing, and no snap layouts flyout appears; the same injected hover and click work on Notepad. A title-bar double-click maximizes but never restores, so a maximized window comes back only by dragging the title bar or Win+↓. Cause: the buttons stop the mouse-down (`src/ui.rs:728`), so GPUI never records the pressed caption button; GPUI's `zoom` only maximizes. **rc.3 retest (2026-10-19):** a title-bar double-click now toggles maximize and restore; the buttons still do nothing and show no snap layouts. The real cause: GPUI answers Windows' hit test with the first control area painted, and the enclosing title-bar Drag area is painted before the buttons, so they hit-test as caption. [0.1.0 stable: Windows caption buttons](../../../../ROADMAP.md). |

## 3. Display scaling

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| D3 | *Windows*: 125 % scale; text crisp, hit targets aligned with what is drawn | Pending | Display scaling is a system setting. *Maintainer*: deferred on 2026-10-19. |
| D4 | *Windows*: 150 % scale; the same, including the popup and dialogs | Pending | As D3. *Maintainer*: deferred on 2026-10-19. |
| D5 | Resizing to the minimum window size keeps every control reachable | Pass | At the minimum (about 720 × 475 px) the header, Markdown/Source switch, Apply and the panel stay visible; Home scrolls to its buttons, Cases and Recents. |

## 4. Files and paths

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| P1 | Open (Mod+O) `Соглашение об урегулировании.docx` from `Документы/Папка с пробелами/`; it converts and its preview shows | Pass | Ctrl+O with the path typed into the dialog; converted, Source preview shown, RU spelling underline on «Маришей». |
| P2 | Save As (Mod+Shift+S) into the same folder under a Cyrillic name with spaces; reopening it from Recents works | Fail | Injected Ctrl+Shift+S opens Save As; `Мировое соглашение копия` (typed without extension) was saved as `…копия.md`, Cyrillic intact. As on macOS it never enters Recents; the tab also keeps its DOCX badge. [After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md). |
| P3 | Dropping that file onto Home opens it | Pass | Maintainer: dropping the file from File Explorer onto Home opened it. |
| P4 | Dropping two files onto the sidebar shows the insertion line and opens both at that position | Pass | Maintainer: two files dropped onto the sidebar showed the insertion line and opened at that position. |
| P5 | Dragging an unsupported file shows "No supported files" and opens nothing | Pass | Maintainer: dropping `образец.xyz` showed "No supported files" and opened nothing. |
| P6 | Dropping a folder opens it as a case | Pass | Maintainer: a dropped folder opened as a case. What a folder drop should offer is [After 0.1.0: Folder drop flow](../../../../ROADMAP.md). |
| P7 | Opening a file from the OS (Finder/Explorer "Open with") opens it in mdoc | Pass | `mdoc.exe "…\reference.pdf"` (what "Open with" runs) opened it with its preview, but in a second mdoc process and window: Windows has no single-instance launch, so every launch restores the session again. The installer registers no file associations ([After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md)). |

## 5. First-use model setup

Run on the clean profile.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| S1 | Offline (network off): Pseudonymize shows the consent card; Download & continue fails cleanly with Retry and no crash | Pending | Needs the network off, a system setting, and the models moved aside again. *Maintainer*: deferred on 2026-10-19. |
| S2 | Online: Retry downloads with byte progress, then Verifying / Checking runtime, and the scan resumes in the requesting tab | Pass | Retry downloaded with byte progress (… / 685 MB), showed "Checking runtime…", and the scan resumed in the requesting tab (24 proposals, 14 identities). Verifying passed too fast to see. |
| S3 | Cancel during download returns to the card; a second attempt completes | Pass | Cancel at 14 / 709 MB returned to the card with "Download cancelled" and Retry; Retry completed. The size then read 685 MB ([After 0.1.0: Dry-run polish](../../../../ROADMAP.md)). |
| S4 | Converting `reference.pdf` with OCR shows the OCR consent card and completes after the download | Pass | `reference.pdf` has a text layer, so the scanned `tests/fixtures/ocr-qualification/russian.pdf` was used: the card warned the default model reads English; Choose model › English & Russian, Download & recognize produced the text exactly, ё included. |
| S5 | Settings shows both models as installed after setup | Pass | Settings shows English & Russian OCR and Standard pseudonymization as Installed. |

## 6. Replacement popup and panel

Fixture: the English demo, scanned.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| R1 | Clicking a proposal opens the popup next to it, inside the window, with buttons inside the popup | Pass | The popup opens below the mention, inside the window, with every control inside it. |
| R2 | Enter applies, ⌫ keeps, ←/→ step through mentions, Mod+↑/↓ jump to the ends, Esc closes | Pass | → 1/6 → 2/6, Ctrl+↓ 6/6, Ctrl+↑ 1/6; Enter applies this mention (`ApplyThis`, ADR 0048); ⌫ keeps it; Esc closes. |
| R3 | Mod+⌫ keeps the same text; Mod+Shift+⌫ keeps the whole entity | Pass | Ctrl+⌫ and Ctrl+Shift+⌫ each kept all six Ms Bennett mentions (Apply 24 → 18); this entity has a single wording. |
| R4 | Editing the alias field and pressing Enter renames the alias | Pass | PERSON_2 → CLAIMANT with Enter; the panel follows. |
| R5 | Mod+Z inside the popup and the panel undoes the last decision; Mod+Shift+Z (and Ctrl+Y on Windows) redoes | Pass | In the popup Ctrl+Z undoes a keep, Ctrl+Y and Ctrl+Shift+Z redo it; Ctrl+Z works in the panel. Undoing an applied mention closes the popup, so its redo then runs from the editor (Ctrl+Y and Ctrl+Shift+Z work there). |
| R6 | Panel list: ↑/↓, Space, Enter, →/←, ⌫ behave as described in ADR 0048 | Pass | From the panel search, ↓ enters the list with a visible focus; ↓ moves, ← goes to the entity, Space toggles keep, ⌫ keeps, Ctrl+Z undoes. |
| R7 | Mod+Enter applies all | Pass | Ctrl+Enter applied all 24 and switched to Share. |
| R8 | Before / After (Mod+\\) shows both columns; Esc leaves | Pass | Maintainer: Ctrl+\ on the keyboard opens Before / After with both columns; Esc leaves. |
| R9 | Alt+Enter, Alt+↑/↓ and Mod+Alt+P work in the editor | Pass | Maintainer: Alt+Enter on the keyboard works in the editor; Alt+↓ and Ctrl+Alt+P as Claude observed. |

## 7. Clipboard

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| C1 | Copy for AI, pasted into the ChatGPT and Claude web apps, arrives as plain text with the placeholder note | Pass | Maintainer: Copy for AI pasted into the ChatGPT and Claude web apps arrives as plain text with the placeholder note. |
| C2 | Copy Markdown of the Russian demo, pasted into TextEdit (plain text mode) or Notepad, is byte-identical to the saved file, Cyrillic intact; the same for a line with CJK text | Pass | Copy Markdown of the Russian demo is byte-identical to its file (3,844 bytes, LF); `cjk.md` (契約 テスト 合意) is byte-identical too. Compared from the clipboard, not through Notepad. |
| C3 | Paste AI Response (Mod+Alt+Shift+V) of a browser selection carrying HTML and plain text uses the plain text and restores aliases | Pass | A clipboard with HTML (bold, red span, an HTML-only word) and plain text, pasted with Ctrl+Alt+Shift+V, opened "Summary. Maya Bennett may settle with Harrow & Finch Ltd; Ms Bennett keeps the reference." |
| C4 | Pseudonymize Clipboard (Mod+Alt+Shift+C) replaces the known case names in the clipboard; pasting elsewhere shows the aliases | Pass | Ctrl+Alt+Shift+C turned "Maya Bennett emailed Harrow & Finch Ltd…; Ms Bennett keeps…" into "PERSON_1 emailed ORG_1…; CLAIMANT keeps…" (names from the active tab; on a tab without names it says so). |
| C5 | Copy, cut and paste between two editor tabs keep the text exactly | Pass | Ctrl+A Ctrl+C in the Russian tab, Ctrl+V into a new tab, Ctrl+A Ctrl+X, Ctrl+V: the clipboard and the new tab's Markdown are byte-identical to the file each time. |

## 8. Keyboard input and IME

Install the Russian layout and the built-in Japanese IME first. The
maintainer does not run the Japanese rows (K3–K7): mark them Unavailable
unless Claude observed them on macOS (ADR 0057).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| K1 | Russian typed into the editor and into the find field appears correctly | Pass | Maintainer: Russian typed with the Russian layout appears correctly in the editor and the find field. |
| K2 | A misspelled Russian word gets the spelling underline; Mod+; and Mod+Shift+; move between misspellings | Pass | Maintainer: the underline shows on misspelled Russian words; Ctrl+; and Ctrl+Shift+; on the keyboard move forward and back. |
| K3 | Japanese in the editor: the marked text is underlined while composing | Unavailable | No Japanese input source; the maintainer does not run the Japanese rows (ADR 0057). |
| K4 | The candidate window opens at the caret, not at the window corner | Unavailable | As K3. |
| K5 | Choosing a candidate commits it; Esc during composition cancels it | Unavailable | As K3. |
| K6 | One undo removes the committed Japanese text | Unavailable | As K3. |
| K7 | Japanese in the find field composes, commits and finds the text | Unavailable | As K3. A headless test composes and commits Japanese in the preview find field. |
| K9 | Dead keys or Option/AltGr characters (e.g. `é`, `€`) type correctly | Pass | Maintainer: AltGr and dead-key characters (`é`, `€`) type correctly. |

## 9. Shortcuts

Every application shortcut, from `bind_app_keys`, `bind_markdown_search_keys`,
`bind_shell_keys`, `pii::ui::bind_keys` and `mdoc_editor::bind_keys`.

| ID | Shortcut | Action | Result | Note |
| --- | --- | --- | --- | --- |
| H1 | Mod+N | New | Pass | New tab opens. |
| H2 | Mod+O | Open | Pass | Open dialog. |
| H3 | Mod+Shift+I | Convert to Markdown | Pass | A CSV (`Name,Count` with a Cyrillic row) opens source-only with the Convert card; Ctrl+Shift+I converted it to a Markdown table, Cyrillic intact. |
| H4 | Mod+S / Mod+Shift+S | Save / Save As | Pass | Ctrl+S wrote the edit to disk; injected Ctrl+Shift+S opens Save As on Windows. |
| H5 | Mod+W | Close tab | Pass | Closes the tab. |
| H6 | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab | Pass | Ctrl+Tab wraps from the last tab to the first; Ctrl+Shift+Tab returns. |
| H7 | Mod+= , Mod+Shift+= , Mod+- , Mod+0 | Zoom in, in, out, reset | Pass | Ctrl+= ×3 → 115 %, Ctrl+Shift+= → +5 %, Ctrl+- → −5 %, Ctrl+0 → 100 %. |
| H8 | Mod+, | Settings; Esc closes, Tab/Shift+Tab move between fields | Pass | Ctrl+, opens Settings; Esc closes; Tab and Shift+Tab move with a visible (subtle) focus. UI Automation keeps reporting the window as focused (see A3). |
| H9 | Mod+K | Palette; ↑/↓, Enter, Esc | Pass | Ctrl+K opens the palette; ↑/↓ move; Enter runs (Copy Markdown, Copy for AI); Esc closes. |
| H10 | Mod+Shift+H | Home | Pass | Ctrl+Shift+H shows Home. |
| H11 | Mod+F, Mod+G, Mod+Shift+G, Enter, Shift+Enter, Esc | Find, next, previous, close | Pass | "agreement": 4/6, Ctrl+G 5/6, Enter 6/6, Shift+Enter 5/6, Ctrl+Shift+G 4/6; Esc closes. |
| H12 | Mod+Shift+P | Pseudonymize | Pass | Ctrl+Shift+P showed the setup card, later scanned the English (14 identities) and Russian (14) demos. |
| H13 | Mod+B, Mod+I, Mod+E, Mod+U, Mod+Shift+X | Bold, italic, code, underline, strike | Pass | Copy Markdown: `*alpha*`, `` `epsilon` ``, `<u>gamma</u>`, `~~delta~~`, bold. A selection ending in a space (from Alt+Shift+←) became `**~~delta~~ **`, which CommonMark does not render as bold ([After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md)). |
| H14 | Mod+A, Mod+C, Mod+X, Mod+V, Mod+Z, Mod+Shift+Z (Ctrl+Y on Windows) | Editing | Pass | Exercised in C5 and R5. |
| H15 | Alt+←/→, Alt+Shift+←/→, Home/End, Shift+arrows, Tab/Shift+Tab | Word moves, selection, indent | Pass | Alt+← and Alt+Shift+←/→ move and select by word; Home/End work. Ctrl+←/→, the Windows word moves, are not bound ([After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md)). |
| H16 | Mod+Q | Quit, with the unsaved-changes prompt | Pass | Ctrl+Q shows a native "Save changes to this document?" dialog (Save, Cancel, Discard) per dirty tab; Discard on each quits. Right after clicking Share › Save as case, Ctrl+Q did nothing until the editor was clicked. |
| H17 | *Windows*: each File-menu action is reachable from the window | Pass | New (Ctrl+N, sidebar, palette), Open… and Open Case… (Home, palette), Save, Save As…, Copy Markdown, Copy for AI, Paste AI Response, Close Tab (toolbar "…" menu); Convert to Markdown appears only in a source-only view (H3). View › Before / After and Toggle Sidebar have no palette entry. |
| H18 | Tab / Shift+Tab move through toolbar and panel controls with a visible focus ring; Esc closes menus | Fail | As on macOS: Tab cycles inside Settings, the popup and the panel list, but from Home it shows no focus and from the editor it indents. [After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md). |

## 10. Case key storage

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| E1 | Nothing asks for the key at launch | Pass | No prompt at launch, on two relaunches or on opening documents. |
| E4 | *Windows*: saving a case asks nothing (DPAPI) | Pass | Share › Save as case saved at once; `cases\key.dpapi` and the `.case` file appeared. |
| E5 | After quit and relaunch, opening a case member restores its replacements without a prompt | Pass | A saved, scanned `Мировое соглашение копия.md` saved as a case; after quit and relaunch, Home › Cases › the case reopened it with its 18 aliases and no prompt. A case saved from an unsaved conversion lists 0 documents and cannot be opened ([After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md)). |

## 11. Screen reader basics

VoiceOver (⌘F5) on macOS, Narrator (Ctrl+Win+Enter) on Windows. The editor's
text is not exposed to assistive technology (known limitation, ADR 0057); do
not mark it Fail.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| A1 | Sidebar toggle, tabs, toolbar buttons and the Pseudonymize control announce a name and role | Pass | UI Automation: Collapse sidebar, Search or run an action, Home, the "Open documents" tab list with named tab items, New Markdown tab, Settings, Theme, Help, More actions, Pseudonymize and Apply are named Buttons/TabItems. Every tab's close button reads only "Close tab". |
| A2 | Replacements panel controls and popup buttons announce a name and role | Pass | Rerun of the rc.1 failure: the popup's scope segments ("This one", "All 6 mentions") and all ten category menu rows are named toggle buttons; panel rows, checkboxes, search, Alias field and Close are named. No unnamed control in the window. |
| A3 | Settings and the Keychain/unsaved-changes dialogs announce their title and buttons; focus stays inside | Fail | The unsaved-changes dialog is a native task dialog: title "Warning", both texts, Save/Cancel/Discard, focus on Save. Settings is not a dialog: no window or title in the tree, the document's controls stay behind it, and Tab focus is not reported to UI Automation. [After 0.1.0: RC acceptance follow-ups](../../../../ROADMAP.md). |
| A4 | Every control in A1–A3 can be reached and activated without a mouse: with the screen reader, or Full Keyboard Access on macOS. Plain Tab from Home or the editor is not required for 0.1.0 (ADR 0057) | Pass | Controls expose Invoke, SelectionItem or Toggle: invoking Collapse/Expand sidebar, selecting a tab and toggling a panel checkbox through UI Automation, as Narrator does, worked. Spoken Narrator output not checked (A5). |
| A5 | Scan status and toasts are announced | Pending | Announcements need spoken output. *Maintainer*: Narrator, deferred on 2026-10-19. |
