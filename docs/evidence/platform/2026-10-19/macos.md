# macOS platform acceptance, 0.1.0-rc.1

The macOS half of the [2026-10-19 run](README.md), from the template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)
([ADR 0057](../../../adr/0057-native-platform-acceptance.md)). Claude ran it
through computer use; rows it could not observe are Pending and marked
*Maintainer*. Shortcuts were injected without a keyboard layout: an ignored
⇧ shortcut is not a failure on its own (see H4).

## Header

| Field | Value |
| --- | --- |
| Date | 2026-10-19 |
| Run by | Claude (computer use, the accessibility tree via AXUIElement, pasteboard reads); rows marked *Maintainer* wait on the maintainer |
| mdoc version and artifact | 0.1.0-rc.1, `mdoc_0.1.0-rc.1_arm64.dmg` (SHA256SUMS verified; the installed app's CDHash `0a5aa6e2…` matches the `.dmg`'s) |
| Machine, CPU, RAM, GPU | Reference MacBook, Apple M4 (10-core GPU), 24 GB |
| OS and build | macOS 15.7.7 (24G720) |
| Displays and scale | Built-in Liquid Retina, 2880 × 1864 Retina; no external display |
| Input sources installed | U.S., ABC, Russian (no Japanese) |

## 1. Install and first launch

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| I1 | *macOS*: the `.dmg` opens; dragging to Applications and launching shows no Gatekeeper warning beyond the standard "downloaded from the Internet" prompt | Pending | `.dmg` holds `mdoc.app` and an Applications link; `spctl` accepts both as Notarized Developer ID. The app was installed before the run, so the first-open prompt was not seen. *Maintainer*: confirm it was only the standard prompt. |
| I2 | *macOS*: `spctl -a -vv /Applications/mdoc.app` reports `Notarized Developer ID` | Pass | `source=Notarized Developer ID`, Tebriz Tagiev (F33DLM8K53); `stapler validate` passes on the `.dmg`. |
| I5 | First launch shows Home with no error, no crash report notice, and no Keychain or network prompt | Pass | Clean profile: Home in Dark (System), no error, crash notice, Keychain or network prompt; no SecurityAgent window. |
| I6 | About/Settings shows the expected version | Pass | mdoc › About mdoc opens Settings › Updates: "mdoc 0.1.0-rc.1 is the latest version." |

## 2. Themes

Fixture: the English demo DOCX, with the Source preview open and aliases applied.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| T1 | Light: shell, sidebar, editor, Replacements panel, popup, dialogs and menus are legible, with no unthemed or invisible controls | Pass | Shell, sidebar, editor, Replacements panel, popup, selection chip, preview and menus legible; no unthemed controls. |
| T2 | Dark: the same | Pass | The same in Dark; the preview pages follow the theme. |
| T3 | System: switching the OS appearance while mdoc is open updates every visible surface without a restart | Pending | Switching the OS appearance is a system setting. *Maintainer*. |
| T4 | Original page tone follows the theme; the override in the Original header works in both themes | Pass | Dark: "Show original paper" switches to white pages and back. Light hides the control because the pages are already original. |
| T5 | Highlights (proposed, applied, search, spelling) stay distinguishable in both themes | Pass | Proposed (category tints), applied (blue), search (yellow, current orange) and spelling (red) are distinct; search checked in Dark only. |
| T6 | *macOS*: traffic lights sit on the unified title row | Pass | Traffic lights sit on the sidebar's top row. |

## 3. Display scaling

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| D1 | *macOS*: Retina display; text and icons are crisp | Pass | Text and icons crisp at 2× (zoomed captures). |
| D2 | *macOS*: a 1× external display; text is legible and the window moves between displays without layout breakage (Unavailable without such a monitor) | Unavailable | No 1× external display at hand. |
| D5 | Resizing to the minimum window size keeps every control reachable | Pass | At the minimum size (about 575 × 385) the header, sidebar and editor controls stay visible; the tab list clips and scrolls above the footer. |

## 4. Files and paths

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| P1 | Open (Mod+O) `Соглашение об урегулировании.docx` from `Документы/Папка с пробелами/`; it converts and its preview shows | Pass | Opened through ⌘O and Go to Folder; converted, Source preview shown, RU spelling underline on «Маришей». |
| P2 | Save As (Mod+Shift+S) into the same folder under a Cyrillic name with spaces; reopening it from Recents works | Fail | Save As (File menu; injected ⌘⇧S is ignored, see H4) wrote `Мировое соглашение копия.md` intact, but it never enters Recents: the Recents row stays the source `.docx`, which reopens as a fresh unsaved conversion. The file opens with ⌘O or a drop and then appears. [After 0.1.0 item 14](../../../../ROADMAP.md). |
| P3 | Dropping that file onto Home opens it | Pass | `Мировое соглашение копия.md` dropped on Home opened in a new tab. |
| P4 | Dropping two files onto the sidebar shows the insertion line and opens both at that position | Pass | Two files dragged between two tabs: the insertion line showed there and both opened at that position. |
| P5 | Dragging an unsupported file shows "No supported files" and opens nothing | Fail | `образец.xyz` over the sidebar shows the insertion line, not "No supported files"; nothing opens on drop. [After 0.1.0 item 13](../../../../ROADMAP.md). |
| P6 | Dropping a folder opens it as a case | Pending | Dropping `Папка с пробелами` opened its documents and showed the Keychain explainer; Not now cancelled, so no case was created. *Maintainer*: rerun with Continue and the login password. |
| P7 | Opening a file from the OS (Finder/Explorer "Open with") opens it in mdoc | Pass | `open -a mdoc reference.pdf` (the Apple Event Finder's Open With sends) opened it in a new tab with its preview. |

## 5. First-use model setup

Run on the clean profile.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| S1 | Offline (network off): Pseudonymize shows the consent card; Download & continue fails cleanly with Retry and no crash | Pending | Needs the network off, a system setting. *Maintainer*. |
| S2 | Online: Retry downloads with byte progress, then Verifying / Checking runtime, and the scan resumes in the requesting tab | Pass | Download & scan downloaded the 665 MB model and the scan resumed in the requesting tab (24 proposals). The Verifying and Checking runtime stages fell in a screen-lock gap and were not seen. |
| S3 | Cancel during download returns to the card; a second attempt completes | Pass | Cancel at 15/665 MB returned to the card with "Download cancelled" and Retry; Retry completed. The size then read 638 MB ([After 0.1.0 item 13](../../../../ROADMAP.md)). |
| S4 | Converting `reference.pdf` with OCR shows the OCR consent card and completes after the download | Pass | `reference.pdf` has a text layer, so the scanned `tests/fixtures/ocr-qualification/russian.pdf` was used: the OCR card warned the default model reads English; Choose model › English & Russian, Download & recognize produced the text exactly, ё included. |
| S5 | Settings shows both models as installed after setup | Pass | Settings shows English & Russian OCR and Standard pseudonymization as Installed. |

## 6. Replacement popup and panel

Fixture: the English demo, scanned.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| R1 | Clicking a proposal opens the popup next to it, inside the window, with buttons inside the popup | Pass | The popup opens below the mention, inside the window, with every control inside the card. |
| R2 | Enter applies, ⌫ keeps, ←/→ step through mentions, Mod+↑/↓ jump to the ends, Esc closes | Pass | →/← step 1/4 ↔ 2/4; Enter applies this mention (`ApplyThis`, ADR 0048); ⌫ keeps it; Esc closes. |
| R3 | Mod+⌫ keeps the same text; Mod+Shift+⌫ keeps the whole entity | Pass | ⌘⌫ and ⇧⌘⌫ each kept all six Ms Bennett mentions; this entity has a single wording, so the two scopes coincide here. |
| R4 | Editing the alias field and pressing Enter renames the alias | Pass | Alias field: PERSON_2 → CLAIMANT with Enter; the panel follows. |
| R5 | Mod+Z inside the popup and the panel undoes the last decision; Mod+Shift+Z redoes | Pass | ⌘Z/⇧⌘Z in the popup (24 → 18 → 24) and ⌘Z in the panel undo and redo decisions. Ctrl+Y is Windows-only. |
| R6 | Panel list: ↑/↓, Space, Enter, →/←, ⌫ behave as described in ADR 0048 | Pass | From the panel search, ↓ enters the list; ↓ moves, ← collapses to the entity, Space toggles keep, ⌫ keeps, ⌘Z undoes. |
| R7 | Mod+Enter applies all | Pass | ⌘↩ applied all 24 and switched to Share. |
| R8 | Before / After (Mod+\\) shows both columns; Esc leaves | Pass | ⌘\ shows Original and Pseudonymized side by side at 1100 pt; a narrow window shows one column with a switcher. Esc leaves. |
| R9 | Alt+Enter, Alt+↑/↓ and Mod+Alt+P work in the editor | Pass | ⌥↩ opens the popup at the caret, ⌥↓ moves to the next proposal, ⌘⌥P turns a selection into a REDACTED_1 proposal. |

## 7. Clipboard

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| C1 | Copy for AI, pasted into the ChatGPT and Claude web apps, arrives as plain text with the placeholder note | Pending | Copy for AI puts plain text only on the pasteboard (no HTML/RTF), starting with the placeholder note and with no original names. *Maintainer*: paste into the ChatGPT and Claude web apps. |
| C2 | Copy Markdown of the Russian demo, pasted into TextEdit (plain text mode) or Notepad, is byte-identical to the saved file, Cyrillic intact; the same for a line with CJK text | Pass | Copy Markdown of the Russian demo is byte-identical to its file (3,844 bytes); a document with a Japanese line (契約 テスト 合意) is byte-identical too. Compared with `pbpaste`, not through TextEdit. |
| C3 | Paste AI Response (Mod+Alt+Shift+V) of a browser selection carrying HTML and plain text uses the plain text and restores aliases | Pass | A pasteboard with HTML (bold, red span) and plain text, pasted with Paste AI reply, restored the plain text: "Summary. Maya Bennett may settle with Harrow & Finch Ltd; Ms Bennett keeps the reference." |
| C4 | Pseudonymize Clipboard (Mod+Alt+Shift+C) replaces the known case names in the clipboard; pasting elsewhere shows the aliases | Pass | ⌘⌥⇧C turned "Maya Bennett emailed Harrow & Finch Ltd…" into "PERSON_1 emailed ORG_1…" (names from the open document; no saved case). |
| C5 | Copy, cut and paste between two editor tabs keep the text exactly | Pass | ⌘A ⌘C in the Russian tab, ⌘V into a new tab: byte-identical Markdown. Cut not exercised. |

## 8. Keyboard input and IME

Install the Russian layout and the built-in Japanese IME first. The
maintainer does not run the Japanese rows (K3–K7): mark them Unavailable
unless Claude observed them on macOS (ADR 0057).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| K1 | Russian typed into the editor and into the find field appears correctly | Pending | Injected Cyrillic reaches the editor and the find field intact; typing with the Russian layout needs a keyboard. *Maintainer*. |
| K2 | A misspelled Russian word gets the spelling underline; Mod+; and Mod+Shift+; move between misspellings | Pending | The spelling underline shows on «Маришей»; ⌘; / ⇧⌘; not tried. *Maintainer*. |
| K3 | Japanese in the editor: the marked text is underlined while composing | Unavailable | No Japanese input source; the maintainer does not run the Japanese rows (ADR 0057). |
| K4 | The candidate window opens at the caret, not at the window corner | Unavailable | As K3. |
| K5 | Choosing a candidate commits it; Esc during composition cancels it | Unavailable | As K3. |
| K6 | One undo removes the committed Japanese text | Unavailable | As K3. |
| K7 | Japanese in the find field composes, commits and finds the text | Unavailable | As K3. A headless test composes and commits Japanese in the preview find field. |
| K8 | *macOS*: Ctrl+⌘+Space opens the character palette and inserts an emoji | Pending | *Maintainer*. |
| K9 | Dead keys or Option/AltGr characters (e.g. `é`, `€`) type correctly | Pending | *Maintainer*. |

## 9. Shortcuts

Every application shortcut, from `bind_app_keys`, `bind_markdown_search_keys`,
`bind_shell_keys`, `pii::ui::bind_keys` and `mdoc_editor::bind_keys`.

| ID | Shortcut | Action | Result | Note |
| --- | --- | --- | --- | --- |
| H1 | Mod+N | New | Pass | New tab opens. |
| H2 | Mod+O | Open | Pass | Open dialog. |
| H3 | Mod+Shift+I | Convert to Markdown | Pending | Acts only in a source-only view (a source opened without conversion); a no-op on Markdown tabs, as coded. Not exercised. |
| H4 | Mod+S / Mod+Shift+S | Save / Save As | Pass | ⌘S saved. Injected ⌘⇧S is ignored (as in the dry run); the maintainer confirmed ⌘⇧S on a physical keyboard on 2026-10-19 (dev build of the RC's code). |
| H5 | Mod+W | Close tab | Pass | Closes the tab. |
| H6 | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab | Pass | ⌃Tab wraps from the last tab to the first; ⌃⇧Tab returns. |
| H7 | Mod+= , Mod+Shift+= , Mod+- , Mod+0 | Zoom in, in, out, reset | Pass | ⌘= twice → 110 %, ⌘- and ⌘0 → 100 %. ⇧⌘= not tried separately. |
| H8 | Mod+, | Settings; Esc closes, Tab/Shift+Tab move between fields | Pass | ⌘, opens Settings; Esc closes; Tab and ⇧Tab move between fields with a visible focus. |
| H9 | Mod+K | Palette; ↑/↓, Enter, Esc | Pass | ⌘K opens the palette; ↓ moves; Esc closes. |
| H10 | Mod+Shift+H | Home | Pass | The maintainer confirmed ⇧⌘H on a physical keyboard on 2026-10-19 (dev build of the RC's code). |
| H11 | Mod+F, Mod+G, Mod+Shift+G, Enter, Shift+Enter, Esc | Find, next, previous, close | Pass | Russian query; ⌘G 4→5, ↩ →6, ⇧↩ →5, ⇧⌘G →4; the view follows; Esc closes. |
| H12 | Mod+Shift+P | Pseudonymize | Pass | ⇧⌘P scanned the Russian document (14 identities). |
| H13 | Mod+B, Mod+I, Mod+E, Mod+U, Mod+Shift+X | Bold, italic, code, underline, strike | Pass | ⌘B, ⌘I, ⌘E, ⌘U (renders as underline; the `<u>` tags show only around the caret) and ⇧⌘X. |
| H14 | Mod+A, Mod+C, Mod+X, Mod+V, Mod+Z, Mod+Shift+Z | Editing | Pass | ⌘A, ⌘C, ⌘V, ⌘Z, ⇧⌘Z exercised in R5 and C5. |
| H15 | Alt+←/→, Alt+Shift+←/→, Home/End, Shift+arrows, Tab/Shift+Tab | Word moves, selection, indent | Pass | ⌥←/→, ⌥⇧←/→ and End. Home, ⇧ arrows and Tab indent not separately checked. ⌘←/→ and ⌘↑/↓ are not bound ([After 0.1.0 item 14](../../../../ROADMAP.md)). |
| H16 | Mod+Q | Quit, with the unsaved-changes prompt | Pass | ⌘Q shows a native "Save changes to this document?" alert per dirty tab (Save, Discard, Cancel); Discard on each quit. |
| H17 | *macOS*: every item in the mdoc, File, Edit, View and Window menus runs its action and shows its shortcut; Edit › Paste works in the find field and in a Save dialog (ADR 0058).  | Pass | All five menus list their items with shortcuts; Settings…, About, Save As…, Copy Markdown, Paste AI Response, Pseudonymize Clipboard, Toggle Sidebar, Find… and the rest ran. Edit › Paste fills the find field and a Save dialog's name (Cyrillic). Toggle Sidebar has no shortcut ([After 0.1.0 item 14](../../../../ROADMAP.md)). |
| H18 | Tab / Shift+Tab move through toolbar and panel controls with a visible focus ring; Esc closes menus | Fail | Tab cycles with a visible focus only once focus is in a control or panel (Settings, popup, panel list). From Home, Tab does nothing (focus stays on the window); from the editor Tab indents, so the toolbar and sidebar cannot be reached by plain Tab. See A4; [After 0.1.0 item 14](../../../../ROADMAP.md). |

## 10. Case key storage

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| E1 | Nothing asks for the key at launch | Pass | No Keychain prompt at launch or on opening documents. |
| E2 | *macOS*: Save mapping as case shows the "mdoc will ask for Keychain access" explainer, then the system dialog; Always Allow saves the case | Pending | The explainer appears on a folder drop and on Save as case; no SecurityAgent window exists before Continue (window list). *Maintainer*: Continue, the login password, Always Allow. |
| E3 | *macOS*: Not now (Esc) cancels the action, and the locked-cases bar offers Unlock… | Pending | Not now (clicked) cancelled and opened no case. An injected Esc right after a Finder drop did nothing, likely because Finder stayed active. No locked-cases bar on a clean profile with no saved cases. *Maintainer*: Esc on the keyboard; the Unlock… bar with the real profile. |
| E5 | After quit and relaunch, opening a case member restores its replacements without a prompt | Pending | Needs a saved case (E2). *Maintainer*. |

## 11. Screen reader basics

VoiceOver (⌘F5) on macOS, Narrator (Ctrl+Win+Enter) on Windows. The editor's
text is not exposed to assistive technology (known limitation, ADR 0057); do
not mark it Fail.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| A1 | Sidebar toggle, tabs, toolbar buttons and the Pseudonymize control announce a name and role | Pass | Accessibility tree: sidebar toggle, search, Home, the "Open documents" tab group with the selected tab, Close tab, New Markdown tab, Settings, "Theme: Dark", Help and toolbar buttons have names and roles; the collapsed rail exposes named tabs. |
| A2 | Replacements panel controls and popup buttons announce a name and role | Fail | Panel checkboxes, search field, rows and most popup controls are named, but the popup's scope segments (This one, All N) were unnamed buttons, as were the category menu rows. Fixed after rc.1 (must-fix: unnamed controls); rerun on rc.2. |
| A3 | Settings and the Keychain/unsaved-changes dialogs announce their title and buttons; focus stays inside | Fail | Settings controls are all named, but Settings is not exposed as a dialog: no title is announced and the document behind stays in the tree ([After 0.1.0 item 14](../../../../ROADMAP.md)). The unsaved-changes alert is native and accessible; the Keychain explainer has a dialog role and title in code. Spoken output not checked. |
| A4 | Every control in A1–A3 can be reached and activated without a mouse | Fail | VoiceOver-style navigation reaches and presses every control (AXPress). Without a screen reader, the toolbar and sidebar are unreachable by keyboard (H18). Deferred: screen readers and Full Keyboard Access reach every control ([ADR 0057](../../../adr/0057-native-platform-acceptance.md), amended; [After 0.1.0 item 14](../../../../ROADMAP.md)). |
| A5 | Scan status and toasts are announced | Pending | Announcements need spoken output. *Maintainer*. |
