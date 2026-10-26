# macOS platform acceptance, 0.2.0-rc.4

The macOS half of the [0.2.0 acceptance run](README.md), from the template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)
([ADR 0057](../../../adr/0057-native-platform-acceptance.md)), on the CI
release candidate `v0.2.0-rc.4` (ROADMAP 0.2.0 must-fix 1). rc.1's macOS run
predates the architecture refactoring
([ADR 0069](../../../adr/0069-app-architecture-and-code-ownership.md)), so the
whole checklist runs again, with S1, A5 and the keyboard-reach follow-ups.

Claude ran it through computer use from the Claude desktop app. State was read
from the accessibility tree (`AXUIElement`), the pasteboard, files on disk and
captures of mdoc's own window; drags from Finder used full-screen control.
Unlike rc.1, `AXFocused` was exposed only for text fields and dialog buttons,
not for other GPUI controls (rc.1's binary, run the same way, behaved the
same), so focus elsewhere was read from the drawn focus indicator. Rows that
need a physical keyboard (section 8, ⇧ shortcuts), spoken VoiceOver output,
the Keychain password or a system setting are **Pending** for the maintainer;
where an injected ⇧ shortcut worked it is noted, but it does not replace the
keyboard check. As the run's prompt said, no model was downloaded: the setup
cards were checked on the clean profile, then the installed models were
copied from the maintainer's profile. Real downloads (S2–S5) are the Windows
run's.

## Header

| Field | Value |
| --- | --- |
| Date | 2026-10-26 |
| Run by | Claude (computer use, the accessibility tree via AXUIElement, pasteboard and window captures); maintainer rows Pending |
| mdoc version and artifact | 0.2.0-rc.4, `mdoc_0.2.0-rc.4_arm64.dmg` from the GitHub pre-release `v0.2.0-rc.4` (tag commit `911dd0b`), SHA-256 `a705fc9d64f266bf8bbf4148d0dc0bb1eb70f61030560bcf482cd547eb79da54` (matches `SHA256SUMS`) |
| Machine, CPU, RAM, GPU | Reference MacBook Air, Apple M4 (10-core GPU), 24 GB |
| OS and build | macOS 15.7.7 (24G720) |
| Displays and scale | Built-in Liquid Retina, 2880 × 1864 Retina (1710 × 1107 pt); no external display |
| Input sources installed | U.S., ABC, Russian (no Japanese) |

## 1. Install and first launch

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| I1 | *macOS*: the `.dmg` opens; dragging to Applications and launching shows no Gatekeeper warning beyond the standard "downloaded from the Internet" prompt | Pass | `gh release download` sets no quarantine, so the `.dmg` got a browser-style `com.apple.quarantine` first. It opens in Finder with `mdoc.app` and an Applications link. A background drag did not land, so Finder copied the app by AppleScript `duplicate` (quarantine kept). First launch showed the standard CoreServicesUIAgent prompt with Cancel / Open (buttons read from the tree; its text was not readable); Open was pressed at the Mac by the maintainer, not by the agent. A relaunch showed no prompt. |
| I2 | *macOS*: `spctl -a -vv /Applications/mdoc.app` reports `Notarized Developer ID` | Pass | `accepted`, `source=Notarized Developer ID`, `Developer ID Application: Tebriz Tagiev (F33DLM8K53)`; `stapler validate` succeeds. |
| I5 | First launch shows Home with no error, no crash report notice, and no Keychain or network prompt | Pass | Clean profile: Home with Open files…, Paste text, Restore AI reply, Pseudonymize clipboard, Open a case…; no SecurityAgent or other prompt window. |
| I6 | About/Settings shows the expected version | Pass | mdoc › About mdoc opens Settings › Updates: "mdoc 0.2.0-rc.4 is the latest version." and "This is a pre-release build, so pre-releases are always included." |

## 2. Themes

Fixture: the English demo DOCX, with the Source preview open and aliases applied.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| T1 | Light: shell, sidebar, editor, Replacements panel, popup, dialogs and menus are legible, with no unthemed or invisible controls | Pass | Shell, sidebar, editor, panel and popup legible; proposals (peach box) and applied aliases (blue) distinct. |
| T2 | Dark: the same | Pass | Shell, sidebar, editor, panel, popup, find bar, Settings, the unsaved-changes dialog, the palette and the Keychain explainer are legible; the preview pages follow the theme. |
| T3 | System: switching the OS appearance while mdoc is open updates every visible surface without a restart | Pending | OS appearance is the maintainer's setting. The theme button cycles System → Light → Dark. |
| T4 | Original page tone follows the theme; the override in the Original header works in both themes | Pass | Dark: "Show original paper" gives white pages, "Show dark pages" goes back. |
| T5 | Highlights (proposed, applied, search, spelling) stay distinguishable in both themes | Pass | Dark: proposed (outlined), applied (blue), search (yellow), current match (orange) and spelling (red squiggle on «Маришей», Russian demo) are distinct. Light: proposed and applied distinct. |
| T6 | *macOS*: traffic lights sit on the unified title row | Pass | On the sidebar's top row, also with the sidebar collapsed. |

## 3. Display scaling

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| D1 | *macOS*: Retina display; text and icons are crisp | Pass | Crisp at 2× in zoomed captures. |
| D2 | *macOS*: a 1× external display; text is legible and the window moves between displays without layout breakage (Unavailable without such a monitor) | Unavailable | No 1× external display at hand. |
| D5 | Resizing to the minimum window size keeps every control reachable | Pass | Minimum 720 × 480. With Find and the Replacements panel open, the find bar wraps to two rows: "Search Markdown" 108 pt wide, Match case, Whole word, Previous / Next match and Close all inside the editor column (rc.1's #115 is fixed). The panel footer's "↻ Rescan" runs about 15 pt past the window's right edge (drawn "Res…") but stays in the tree and clickable (Other observations). |

## 4. Files and paths

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| P1 | Open (Mod+O) `Соглашение об урегулировании.docx` from `Документы/Папка с пробелами/`; it converts and its preview shows | Pass | ⌘O opened the Open panel; Go to Folder with the Cyrillic path (set through the accessibility tree, since injected keystrokes can't type Cyrillic) opened it, converted, with the preview. |
| P2 | Save As (Mod+Shift+S) into the same folder under a Cyrillic name with spaces; reopening it from Recents works | Pass | File › Save As… defaulted to "Папка с пробелами"; saved "Копия соглашения с пробелами.md"; after ⌘W, Home's Recents row "Копия соглашения с пробелами.md in Папка с пробелами, opened just now" reopened it. ⇧⌘S by hand is Pending (H4). |
| P3 | Dropping that file onto Home opens it | Pass | Finder drag over Home: the zone turns green, "Drop to open"; dropped, it opened. |
| P4 | Dropping two files onto the sidebar shows the insertion line and opens both at that position | Pass | Two `.md` files: the insertion line showed between `ru-demo.md` and `russian.pdf`, and both opened there. |
| P5 | Dragging an unsupported file over Home or the sidebar says "These files can't be opened" (no insertion line) and opens nothing | Pass | `unsupported.xyz` over Home: red zone, "These files can't be opened"; over the sidebar: the same message, no insertion line; released, nothing opened. |
| P6 | Dropping a folder opens it as a case | Pending | Dropping "Case folder" on Home opened both files and the "mdoc will ask for Keychain access" explainer; creating the case needs Continue and the Keychain password. |
| P7 | Opening a file from the OS (Finder/Explorer "Open with") opens it in mdoc | Fail | `.md`, `.docx` and the text-layer `reference.pdf` open from Open With (`open -a`, the same open-documents event). A scanned PDF that needs OCR (`tests/fixtures/ocr-qualification/russian.pdf`, `english.pdf`, a copy) opens nothing: no tab, no card, no message, 6 tries, with and without the OCR model. The same files open with the OCR card through File › Open and a drop on Home. [Opening a scanned PDF from Finder (Open With) does nothing](https://github.com/lawkitt/mdoc/issues/166). |

## 5. First-use model setup

Run on the clean profile.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| S1 | Offline (network off): Pseudonymize shows the consent card; Download & continue fails cleanly with Retry and no crash | Pending | Turning the network off is the maintainer's. Online, on the clean profile, Pseudonymize shows "Set up pseudonymization" with Download & scan, Cancel, Choose model… and "Standard model · 665 MB download, once"; Cancel returns to the document. |
| S2 | Online: Retry downloads with byte progress, then Verifying / Checking runtime, and the scan resumes in the requesting tab | Unavailable | No model download on the Mac (the run's prompt: models copied from the maintainer's profile); covered by the Windows run. |
| S3 | Cancel during download returns to the card; a second attempt completes | Unavailable | As S2. |
| S4 | Converting `reference.pdf` with OCR shows the OCR consent card and completes after the download | Unavailable | `reference.pdf` has a text layer, so `tests/fixtures/ocr-qualification/russian.pdf` was opened (File › Open): the card with Recognition languages English / English & Russian (English & Russian preselected), Download & recognize, Use native text only, Choose model… and "PP-OCRv5 Cyrillic + v6 detector". Not downloaded (as S2). |
| S5 | Settings shows both models as installed after setup | Unavailable | Setup was not run. With the models copied in, Settings shows English, English & Russian and Standard as Installed and Full precision as "Download · 1246 MB". |

## 6. Replacement popup and panel

Fixture: the English demo, scanned (14 identities, 24 to replace).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| R1 | Clicking a proposal opens the popup next to it, inside the window, with buttons inside the popup | Pass | Below "Ms Bennett", inside the window, every control inside the card. |
| R2 | Enter applies in the shown segment (All N, then This one) without changing it and opens the next proposal; ⌫ keeps, ←/→ step through mentions, Mod+↑/↓ jump to the ends, Esc closes | Pass | Enter with All 6 applied six ("6 applied") and opened "Mia"; ⌫ kept it ("1 kept"); Esc closes. "Harrow & Finch Ltd" Mention 2 of 4: → 3, ← 2, ⌘↓ 4, ⌘↑ 1 (the counter is in the tree). |
| R3 | Mod+⌫ keeps the same text; Mod+Shift+⌫ keeps the whole entity | Pass | ⌘⌫ kept all 4 "Harrow & Finch Ltd" mentions. Injected ⇧⌘⌫ kept the entity; Pending by hand. |
| R4 | Editing the alias field and pressing Enter renames the alias | Pass | ORG_1 → EMPLOYER; the panel row follows. |
| R5 | Mod+Z inside the popup and the panel undoes the last decision; Mod+Shift+Z (and Ctrl+Y on Windows) redoes | Pass | ⌘Z in the popup undid a keep, injected ⇧⌘Z redid it (Pending by hand); ⌘Z in the panel undid an entity keep. |
| R6 | Panel list: ↑/↓, Space, Enter, →/←, ⌫ behave as described in ADR 0048 | Pass | From the search, ↓ enters the list with a ring; Space keeps and unkeeps; → expands, ← collapses; ⌫ keeps the entity; Enter reveals the mention in the editor. |
| R7 | Mod+Enter and the toolbar's Apply N apply all and close the popup; one Mod+Z undoes them | Pass | ⌘↩ with the popup open applied all ("23 replaced · 1 kept"), closed it and switched to Share; one ⌘Z went back to Apply 17; the toolbar's Apply 17 applied all. Once, ⌘↩ right after renaming an alias left ⌘Z inert until the caret was put in the editor, as rc.1 saw after Apply N (Other observations). |
| R8 | Before / After (Mod+\\) shows both columns; Esc leaves | Pass | With the panel hidden, Original and Pseudonymized side by side; with the panel open there is room for one column, with an Original / Pseudonymized switch. Esc leaves. |
| R9 | Alt+Enter, Alt+↑/↓ and Mod+Alt+P work in the editor | Pass | From the caret, ⌥↓ opened the next mention (EMPLOYER 2 of 4), ⌥↩ opened at the caret, ⌥↑ the previous one (PERSON_2 1 of 6); ⌘⌥P on "colleagues" made a new proposal (REDACTED_1), undone. |

## 7. Clipboard

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| C1 | Copy for AI, pasted into the ChatGPT and Claude web apps, arrives as plain text with the placeholder note | Pending | File › Copy for AI puts plain text only on the pasteboard (`public.utf8-plain-text`), starting with the placeholder note and `---`; names are aliases except one proposal deliberately left unapplied. Pasting into the web apps needs a browser: the maintainer's. |
| C2 | Copy Markdown of the Russian demo, pasted into TextEdit (plain text mode) or Notepad, is byte-identical to the saved file, Cyrillic intact; the same for a line with CJK text | Pass | Copy Markdown of the Russian demo equals the file (3,844 bytes, `cmp`); a line "日本語のテスト 中文 한국어 — Cyrillic: Привет" pasted into a new tab and copied back is identical. Compared with `pbpaste`, not through TextEdit. |
| C3 | Paste AI Response (Mod+Alt+Shift+V) of a browser selection carrying HTML and plain text uses the plain text and restores aliases | Pass | A pasteboard with HTML (`<b>HTML-ONLY</b> …`) and plain text "PERSON_2 should accept the EMPLOYER offer of £18,500."; File › Paste AI Response opened "Restored – Settlement agreement.md" with "Ms Bennett should accept the Harrow & Finch Ltd offer of £18,500." (plain text, aliases restored). ⌥⇧⌘V by hand is Pending. |
| C4 | Pseudonymize Clipboard (Mod+Alt+Shift+C) replaces the known case names in the clipboard; pasting elsewhere shows the aliases | Pass | From the Settlement tab, Edit › Pseudonymize Clipboard turned "Ms Bennett met Harrow & Finch Ltd in Bristol." into "PERSON_2 met EMPLOYER in Bristol." with the bar "Replaced 2 names from this document in the clipboard…". ⌥⇧⌘C by hand is Pending. |
| C5 | Copy, cut and paste between two editor tabs keep the text exactly | Pass | ⌘A ⌘C in one tab, ⌘V in a new tab: identical; ⌘A ⌘X put the text on the pasteboard and emptied the tab; ⌘Z restored it. |

## 8. Keyboard input and IME

Install the Russian layout and the built-in Japanese IME first. The
maintainer does not run the Japanese rows (K3–K7): mark them Unavailable
unless Claude observed them on macOS (ADR 0057).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| K1 | Russian typed into the editor and into the find field appears correctly | Pending | Physical keyboard. Injected keystrokes can't type Cyrillic; Cyrillic set through the tree reached the Open and Save panels intact (P1, P2). |
| K2 | A misspelled Russian word gets the spelling underline; Mod+; and Mod+Shift+; move between misspellings | Pending | Physical keyboard. The underline shows on «Маришей». |
| K3 | Japanese in the editor: the marked text is underlined while composing | Unavailable | No Japanese input source; the maintainer does not run the Japanese rows (ADR 0057). |
| K4 | The candidate window opens at the caret, not at the window corner | Unavailable | As K3. |
| K5 | Choosing a candidate commits it; Esc during composition cancels it | Unavailable | As K3. |
| K6 | One undo removes the committed Japanese text | Unavailable | As K3. |
| K7 | Japanese in the find field composes, commits and finds the text | Unavailable | As K3. |
| K8 | *macOS*: Ctrl+⌘+Space opens the character palette and inserts an emoji | Pending | Physical keyboard. Edit › Emoji & Symbols is in the menu. |
| K9 | Dead keys or Option/AltGr characters (e.g. `é`, `€`) type correctly | Pending | Physical keyboard. |

## 9. Shortcuts

Every application shortcut, from `bind_app_keys`, `bind_markdown_search_keys`,
`bind_shell_keys`, `pii::ui::bind_keys` and `mdoc_editor::bind_keys`.

| ID | Shortcut | Action | Result | Note |
| --- | --- | --- | --- | --- |
| H1 | Mod+N | New | Pass | New Untitled.md tab. |
| H2 | Mod+O | Open | Pass | Open panel (P1). |
| H3 | Mod+Shift+I | Convert to Markdown | Pending | ⇧ shortcut; listed in File with ⇧⌘I. |
| H4 | Mod+S / Mod+Shift+S | Save / Save As | Pass | ⌘S saved an edited `.md` to disk and cleared the unsaved dot. ⇧⌘S is Pending (Save As ran from the File menu, P2). |
| H5 | Mod+W | Close tab | Pass | Closed the saved Cyrillic copy at once (P2). |
| H6 | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab | Pass | ⌃Tab from the last tab wrapped to the first, then the next; injected ⌃⇧Tab went back (confirm by hand). |
| H7 | Mod+= , Mod+Shift+= , Mod+- , Mod+0 | Zoom in, in, out, reset | Pass | ⌘= 105 %, ⇧⌘= 110 % (injected; Pending by hand), ⌘- 105 %, ⌘0 100 %. |
| H8 | Mod+, | Settings; Esc closes, Tab/Shift+Tab move between fields | Pass | Tab and ⇧Tab move a visible ring (English › Details); F6 stays inside; Esc closes. |
| H9 | Mod+K | Palette; ↑/↓, Enter, Esc | Pass | ⌘K focuses the palette field; "settings" lists Go to › Settings ⌘,; Enter opened Settings. ↑/↓ and Esc were not rechecked on the RC (they passed on rc.1). |
| H10 | Mod+Shift+H | Home | Pending | ⇧ shortcut; injected ⇧⌘H went Home. |
| H11 | Mod+F, Mod+G, Mod+Shift+G, Enter, Shift+Enter, Esc | Find, next, previous, close | Pass | "the" Match 2 of 16; Enter 3, Enter 4, ⌘G 5, ⇧⌘G 4, ⇧↩ 3; Esc closes. The ⇧ keys were injected; confirm by hand. |
| H12 | Mod+Shift+P | Pseudonymize | Pending | ⇧ shortcut; injected ⇧⌘P scanned the English demo (Review · 14, Apply 24). |
| H13 | Mod+B, Mod+I, Mod+E, Mod+U, Mod+Shift+X | Bold, italic, code, underline, strike | Pass | `**five**`, `*three*`, `` `one` ``, `<u>gamma</u>`, `~~alpha~~` in the copied Markdown. ⇧⌘X was injected; Pending by hand. |
| H14 | Mod+A, Mod+C, Mod+X, Mod+V, Mod+Z, Mod+Shift+Z (Ctrl+Y on Windows) | Editing | Pass | ⌘A, ⌘C, ⌘X, ⌘V and ⌘Z as in C5; injected ⇧⌘Z redid the cut (Pending by hand). |
| H15 | Alt+←/→, Alt+Shift+←/→, Home/End, Shift+arrows, Tab/Shift+Tab | Word moves, selection, indent | Pass | ⌥←/→ move by word, Home / End to the line ends, ⇧← selects, Tab nests a list item and ⇧Tab outdents it (copied Markdown). The ⇧ keys were injected; confirm by hand. |
| H16 | Mod+Q | Quit, with the unsaved-changes prompt | Pass | ⌘Q asked "Save changes to “name”?" (Save, Discard, Cancel) for each of four dirty tabs; Discard on each quit. |
| H17 | *macOS*: every item in the mdoc, File, Edit, View and Window menus runs its action and shows its shortcut; Edit › Paste works in the find field and in a Save dialog (ADR 0058) | Pass | All five menus list their shortcuts (for example Paste AI Response ⌥⇧⌘V, Pseudonymize Clipboard ⌥⇧⌘C, Toggle Sidebar ⌃⌘S, Before / After ⌘\\, Next Tab ⌃Tab). About, Settings…, Save As…, Copy Markdown, Copy for AI, Paste AI Response, Pseudonymize Clipboard, Find…, Toggle Sidebar, Next Tab and Quit ran. Edit › Paste filled the find field and the Save dialog's name field. |
| H18 | F6 / Shift+F6 cycle sidebar, toolbar, editor, Replacements, Original and Comments (open ones only), returning to the last control in each; Tab / Shift+Tab move between controls with a visible focus ring and, at a region's last (first) control, on to the next (previous) region, so Tab past the toolbar enters the editor; in the sidebar ↑/↓ move between tabs, ⌫ closes one and Shift+F10 opens its case actions; the category list keeps focus until closed; Esc closes menus and returns from the sidebar or toolbar to the editor (ADR 0062) | Pending | Checked: F6 goes editor → Replacements (caret in the panel search) → Original (ring on its ×) → sidebar (back to Settings, its last control) → toolbar → editor, skipping Original when it is hidden. In the toolbar, focus shows as a filled button; Enter toggled the original, Tab moved to "…", Enter opened More actions, Esc closed it. Not checked: sidebar ↑/↓, ⌫ and ⇧F10 on a tab row, because injected keys never put focus on a row (F6 returns to Settings) and `AXFocused` was not exposed; physical keyboard. The category list needs a saved case. |
| H19 | Enter, Shift+Enter in the editor | New paragraph, line break; Copy for AI pasted into a chat keeps both apart (ADR 0060) | Pass | "line one" ⇧↩ "line two" ↩ "para two" copies as `line one\` + newline + `line two`, blank line, `para two`, from Copy Markdown and Copy for AI. Pasting into a chat is Pending (C1). |
| H20 | *macOS*: ⌘←/→ on a wrapped paragraph and a list item, ⌘↑/↓, each with ⇧ | Visual line start / end (after a list marker first), document start / end; ⇧ selects (ADR 0061) | Pass | On a paragraph wrapping to several rows, ⌘← went to the start of the caret's visual row (a typed marker landed mid-paragraph); ⌘← on a list item lands after `- `; ⌘↑ / ⌘↓ reach the document's start and end. The ⇧ variants are Pending. |
| H21 | *macOS*: ⌥⌫, ⌥⌦, ⌘⌫ | Delete word left, word right, to the line start | Pass | ⌥⌫ deleted "delta", ⌥⌦ at the line start deleted "alpha", ⌘⌫ emptied the line (copied Markdown). |

### Keyboard reach follow-ups (ROADMAP 0.2.0 must-fix 1)

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| F6-explainer | F6 stays inside the Keychain explainer | Pass | F6 left the ring on Continue; nothing outside the explainer took focus. |
| Explainer Tab | Tab and ⇧Tab cycle the Keychain explainer's two buttons | Pass | Focus starts on Not now; Tab → Continue → Not now; ⇧Tab → Continue (ring in captures). |
| ⇧F10 | Shift+F10 opens a sidebar tab's case actions in both sidebar layouts | Pending | A blocking maintainer row. Injected ⇧F10 opened nothing because focus could not be put on a tab row by injected keys (H18). |
| Categories | "Categories to detect" takes focus, holds Tab and returns focus on Esc or Close | Pending | Needs a saved case, and saving one needs the Keychain password. |
| Recents names | VoiceOver reads each Home Recents row's name | Pass | Each row is a named button, for example "Копия соглашения с пробелами.md in Папка с пробелами, opened just now" or "Settlement agreement.docx in mdoc-acceptance-fixtures, pseudonymized, opened 9 min ago"; it reopened the file (P2). Spoken output is the maintainer's (A5 list). |

## 10. Case key storage

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| E1 | Nothing asks for the key at launch | Pass | No prompt at first launch, on two relaunches with restored tabs, or on opening documents; no SecurityAgent window over 10 s. |
| E2 | *macOS*: Save mapping as case shows the "mdoc will ask for Keychain access" explainer, then the system dialog; Always Allow saves the case | Pending | The explainer appears on a folder drop, with its title and a captioned drawing of the macOS dialog; no SecurityAgent window before Continue. The system dialog needs the Keychain password. |
| E3 | *macOS*: Not now (Esc) cancels the action, and the locked-cases bar offers Unlock… | Pass | Esc closed the explainer and created no case (no `cases` folder); the files stay open as plain tabs. The Unlock… bar needs locked cases (Pending). |
| E5 | After quit and relaunch, opening a case member restores its replacements without a prompt | Pending | Needs a saved case. |

## 11. Screen reader basics

VoiceOver (⌘F5) on macOS, Narrator (Ctrl+Win+Enter) on Windows. The editor's
text is not exposed to assistive technology (known limitation, ADR 0057); do
not mark it Fail.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| A1 | Sidebar toggle, tabs, toolbar buttons and the Pseudonymize control announce a name and role | Pass | Accessibility tree: Collapse sidebar, Search or run an action, Home, the "Open documents" tab group with named tabs and "Close <name>" buttons, New Markdown tab, Settings, "Theme: System", Help, Hide replacements, Hide the original file, More actions, Pseudonymize / Apply N / Share, the zoom buttons, the preview toolbar and Home's buttons and Recents rows are named. |
| A2 | Replacements panel controls and popup buttons announce a name and role | Pass | Panel tabs "Review · 14" / Share, rows ("Ms Bennett to PERSON_2, 6 mentions, 0 applied, 0 kept, collapsed"), "Replace … with …" checkboxes, the search field; popup "Replace · untick to keep 4 originals", Alias field, "Correct category for selected scope", "This one" / "All 4 mentions", Previous / Next mention, "Mention 2 of 4" and "Close replacement popup". |
| A3 | Settings and the Keychain/unsaved-changes dialogs announce their title and buttons; focus stays inside | Pass | Settings and "Save changes to “Settlement agreement.md”?" are AXDialogs with their titles and named buttons (Save focused); the explainer shows Not now / Continue. Tab and F6 stay inside Settings and the explainer. Spoken output is Pending. |
| A4 | Every control in A1–A3 can be reached and activated by keyboard alone (F6, Tab, arrows, Enter, Space), from Home and from the editor, without a screen reader or Full Keyboard Access (ADR 0062) | Pending | Reached by keys: the panel (R6), popup (R2, R9), toolbar and More actions (H18), Settings (H8), the explainer and the unsaved-changes dialog. The sidebar's tab rows and Home's Recents rows were not reached by injected keys (H18): physical keyboard. |
| A5 | Scan status and toasts are announced | Pending | Spoken output is the maintainer's. |

## Other observations

Not checklist failures; for the maintainer to triage.

- At 720 × 480 with the Replacements panel open, the panel footer's
  "↻ Rescan" runs about 15 pt past the window's right edge and is drawn
  "Res…"; it stays in the tree and its visible part is clickable.
- The toolbar shows keyboard focus as a filled button. On the original-file
  toggle, the "on" fill looks the same, so focus on it can't be seen while the
  original is shown.
- After ⌘↩ (apply all) pressed straight after renaming an alias, ⌘Z did
  nothing and Edit › Undo was disabled until the caret was put in the editor;
  from a plain popup, ⌘↩ then ⌘Z worked. rc.1 noted the same after Apply N.
- After dropping two files into the sidebar, the hover tooltip over the new
  `two-b.md` row still named the previous row (`russian.pdf`).
- ⌘Q with pseudonymized, unsaved tabs asked only about saving changes, not
  about the mapping (known: "One close prompt", 0.3.0, #119).
