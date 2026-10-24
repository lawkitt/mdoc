# macOS platform acceptance, 0.2.0-rc.1

The macOS half of the [0.2.0 acceptance run](README.md), from the template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)
([ADR 0057](../../../adr/0057-native-platform-acceptance.md)), on the CI
release candidate `v0.2.0-rc.1`. It includes S1 and A5, deferred from 0.1.0,
and the keyboard-reach follow-ups in ROADMAP 0.2.0 must-fix 1.

Claude ran it through computer use from the Claude desktop app, reading state
from the accessibility tree (`AXUIElement`, including `AXFocused`) and the
pasteboard. Rows that need a physical keyboard (section 8, ⇧ shortcuts),
spoken VoiceOver output, the Keychain password or a system setting are
**Pending** for the maintainer; where an injected ⇧ shortcut happened to work
it is noted, but it does not replace the keyboard check. The network was slow,
so no model was downloaded: the setup cards were checked on the clean profile,
then the installed models were copied from the maintainer's profile for the
popup, theme and clipboard rows. Real downloads are covered by the Windows run.

## Header

| Field | Value |
| --- | --- |
| Date | 2026-10-24 |
| Run by | Claude (computer use, the accessibility tree via AXUIElement, pasteboard reads); maintainer rows Pending |
| mdoc version and artifact | 0.2.0-rc.1, `mdoc_0.2.0-rc.1_arm64.dmg` from the draft GitHub Release `v0.2.0-rc.1` (tag commit `c3dc4ff`), SHA-256 `0c70cf2cb1dcfc18289a2ea9678062f245a29c71e1f91aa06ce580b2ac7da709` (matches `SHA256SUMS`) |
| Machine, CPU, RAM, GPU | Reference MacBook Air, Apple M4 (10-core GPU), 24 GB |
| OS and build | macOS 15.7.7 (24G720) |
| Displays and scale | Built-in Liquid Retina, 2880 × 1864 Retina; no external display |
| Input sources installed | U.S., ABC, Russian (no Japanese) |

## 1. Install and first launch

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| I1 | *macOS*: the `.dmg` opens; dragging to Applications and launching shows no Gatekeeper warning beyond the standard "downloaded from the Internet" prompt | Pass | `gh release download` sets no quarantine, so the `.dmg` was given a browser-style `com.apple.quarantine` attribute first. The `.dmg` holds `mdoc.app` and an Applications link; Finder drag copied it with quarantine; launch showed only "“mdoc” is an app downloaded from the Internet. Are you sure you want to open it?" (Cancel / Open); Open launched it. |
| I2 | *macOS*: `spctl -a -vv /Applications/mdoc.app` reports `Notarized Developer ID` | Pass | `accepted`, `source=Notarized Developer ID`, team `F33DLM8K53`; `stapler validate` succeeds. |
| I5 | First launch shows Home with no error, no crash report notice, and no Keychain or network prompt | Pass | Clean profile: Home, no error, crash notice or prompt; no SecurityAgent window in the window list. |
| I6 | About/Settings shows the expected version | Pass | mdoc › About mdoc opens Settings › Updates: "mdoc 0.2.0-rc.1 is the latest version." and "This is a pre-release build, so pre-releases are always included." |

## 2. Themes

Fixture: the English demo DOCX, with the Source preview open and aliases applied.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| T1 | Light: shell, sidebar, editor, Replacements panel, popup, dialogs and menus are legible, with no unthemed or invisible controls | Pass | Shell, sidebar, editor, panel, popup on an applied alias, Settings, the "Keep the replacement mapping?" and unsaved-changes dialogs, the palette and the menus are legible. |
| T2 | Dark: the same | Pass | The same in Dark; the preview pages follow the theme. |
| T3 | System: switching the OS appearance while mdoc is open updates every visible surface without a restart | Pending | OS appearance is the maintainer's setting. The theme button cycles Light → Dark → System; System followed the OS (Dark). |
| T4 | Original page tone follows the theme; the override in the Original header works in both themes | Pass | Dark: "Show original paper" gives white pages and back to dark pages. Light shows original paper already and the header has no tone toggle, as in the dry run. |
| T5 | Highlights (proposed, applied, search, spelling) stay distinguishable in both themes | Pass | Proposed (category underline tints), applied (blue box), search (yellow, current orange) and spelling (red squiggle, Russian demo) are distinct in Light and Dark. |
| T6 | *macOS*: traffic lights sit on the unified title row | Pass | On the sidebar's top row, also with the sidebar collapsed. |

## 3. Display scaling

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| D1 | *macOS*: Retina display; text and icons are crisp | Pass | Crisp at 2× in zoomed captures. |
| D2 | *macOS*: a 1× external display; text is legible and the window moves between displays without layout breakage (Unavailable without such a monitor) | Unavailable | No 1× external display at hand. |
| D5 | Resizing to the minimum window size keeps every control reachable | Fail | Minimum is 720 × 480. Header, sidebar, panel and Before/After controls stay visible, but with Find open and the Replacements panel open, the find bar's "Search Markdown" field is 0 pt wide and Next match / Close Markdown search lie under the panel (accessibility frames). ⌘G and Esc still work. [Find bar controls hidden under the Replacements panel at the minimum window size](https://github.com/lawkitt/mdoc/issues/115). |

## 4. Files and paths

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| P1 | Open (Mod+O) `Соглашение об урегулировании.docx` from `Документы/Папка с пробелами/`; it converts and its preview shows | Pass | ⌘O; Go to Folder in the Open panel took injected keys this time; converted, Source preview shown, RU spelling underline on «Марищей». |
| P2 | Save As (Mod+Shift+S) into the same folder under a Cyrillic name with spaces; reopening it from Recents works | Pass | File › Save As… wrote `Мировое соглашение копия.md` (name pasted with Edit › Paste); the tab badge became MD. After closing the tab, Home › Tab to its Recents row › Return reopened it. Injected ⇧⌘S did nothing: Pending. |
| P3 | Dropping that file onto Home opens it | Pass | From Finder onto Home's drop zone: opens in a new tab. |
| P4 | Dropping two files onto the sidebar shows the insertion line and opens both at that position | Pass | `legal-opinion.docx` and `reference.pdf` between `russian.pdf` and Settlement agreement: insertion line, both opened there. |
| P5 | Dragging an unsupported file over Home or the sidebar says "These files can't be opened" (no insertion line) and opens nothing | Pass | `образец.xyz` over Home: red drop zone, "These files can't be opened"; over the sidebar: the same message, no insertion line; the drop opened nothing. |
| P6 | Dropping a folder opens it as a case | Pass | Dropping `Папка с пробелами` opened its documents and showed the Keychain explainer; Esc cancelled. Creating the case needs the Keychain password (Pending with E2). |
| P7 | Opening a file from the OS (Finder/Explorer "Open with") opens it in mdoc | Pass | `open -a mdoc russian.pdf` opened it in a new tab. |

## 5. First-use model setup

Run on the clean profile, before the models were copied in.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| S1 | Offline (network off): Pseudonymize shows the consent card; Download & continue fails cleanly with Retry and no crash | Pending | Turning the network off is the maintainer's. Online, on the clean profile, ⇧⌘P (injected) shows "Set up pseudonymization" with Download & scan, Cancel, Choose model… and "Standard model · 665 MB download, once"; Cancel returns to the document. |
| S2 | Online: Retry downloads with byte progress, then Verifying / Checking runtime, and the scan resumes in the requesting tab | Unavailable | Slow network; no model was downloaded on this Mac by agreement. Covered by the Windows run. |
| S3 | Cancel during download returns to the card; a second attempt completes | Unavailable | As S2. |
| S4 | Converting `reference.pdf` with OCR shows the OCR consent card and completes after the download | Unavailable | `reference.pdf` has a text layer, so `tests/fixtures/ocr-qualification/russian.pdf` was used: "Some pages need text recognition", Recognize English / English & Russian (English & Russian preselected: "the file name mentions Russian"), Download & recognize, Use native text only, "English & Russian OCR · 54 MB download". Not downloaded (as S2). |
| S5 | Settings shows both models as installed after setup | Unavailable | Setup was not run. With the models copied in, Settings shows English text recognition and Standard pseudonymization as Installed. |

## 6. Replacement popup and panel

Fixture: the English demo, scanned (14 identities, 24 to replace).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| R1 | Clicking a proposal opens the popup next to it, inside the window, with buttons inside the popup | Pass | Below the mention, inside the window, every control inside the card. |
| R2 | Enter applies in the shown segment (All N, then This one) without changing it and opens the next proposal; ⌫ keeps, ←/→ step through mentions, Mod+↑/↓ jump to the ends, Esc closes | Pass | "Ms Bennett" 1/6: → moved to 2/6 with the popup at the mention (the dry run's scroll-to-end, #102, is fixed); ⌘↓ 6/6, ⌘↑ 1/6; Enter with All 6 applied six and opened "Mia", still on screen; ⌫ kept it ("Kept 1 · Undo"); Esc closes. |
| R3 | Mod+⌫ keeps the same text; Mod+Shift+⌫ keeps the whole entity | Pass | ⌘⌫ kept all 4 "Harrow & Finch Ltd" mentions. Injected ⇧⌘⌫ kept the entity; Pending by hand. |
| R4 | Editing the alias field and pressing Enter renames the alias | Pass | PERSON_3 → COLLEAGUE; the panel follows. |
| R5 | Mod+Z inside the popup and the panel undoes the last decision; Mod+Shift+Z (and Ctrl+Y on Windows) redoes | Pass | ⌘Z in the popup undid a keep; ⌘Z in the panel undid an entity keep. Injected ⇧⌘Z redid; Pending by hand. |
| R6 | Panel list: ↑/↓, Space, Enter, →/←, ⌫ behave as described in ADR 0048 | Pass | From the search, ↓ enters the list with a ring; Space toggles keep; ← collapses, → expands; ⌫ keeps the entity; Enter reveals the mention in the editor. |
| R7 | Mod+Enter and the toolbar's Apply N apply all and close the popup; one Mod+Z undoes them | Pass | ⌘↩ with the popup open on "Daniel Bennett" applied 24, closed it and switched to Share; one ⌘Z went back to 18 to replace; the toolbar's Apply 18 applied all. |
| R8 | Before / After (Mod+\\) shows both columns; Esc leaves | Pass | Original and Pseudonymized side by side; Esc leaves. |
| R9 | Alt+Enter, Alt+↑/↓ and Mod+Alt+P work in the editor | Pass | ⌥↩ opens the popup at the caret; ⌥↓ from the caret opens the next mention (4/6); ⌘⌥P on "should" made a new proposal (REDACTED_1, Other). |

## 7. Clipboard

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| C1 | Copy for AI, pasted into the ChatGPT and Claude web apps, arrives as plain text with the placeholder note | Pending | From the Share panel and File › Copy for AI the pasteboard holds plain text only (`public.utf8-plain-text`), starts with the placeholder note and has no original names. Pasting into the web apps needs a browser: the maintainer's. |
| C2 | Copy Markdown of the Russian demo, pasted into TextEdit (plain text mode) or Notepad, is byte-identical to the saved file, Cyrillic intact; the same for a line with CJK text | Pass | Copy Markdown of `Соглашение об урегулировании.md` equals the file (3,844 bytes); a saved `cjk line.md` with 契約 テスト 合意 equals its copy (33 bytes). Compared with `pbpaste` and `cmp`, not through TextEdit. |
| C3 | Paste AI Response (Mod+Alt+Shift+V) of a browser selection carrying HTML and plain text uses the plain text and restores aliases | Pass | A pasteboard with HTML (bold, red span) and plain text; injected ⌥⇧⌘V opened "Restored – Settlement agreement.md" with plain text and "Maya Bennett … Harrow & Finch Ltd …" restored ("Restored 3 mentions of 3 values"). ⌥⇧⌘V by hand is Pending. |
| C4 | Pseudonymize Clipboard (Mod+Alt+Shift+C) replaces the known case names in the clipboard; pasting elsewhere shows the aliases | Pass | From the Settlement tab, injected ⌥⇧⌘C turned "Maya Bennett emailed Harrow & Finch Ltd about Priya Shah." into "PERSON_1 emailed ORG_1 about PERSON_6." (names from the open document; no saved case). ⌥⇧⌘C by hand is Pending. |
| C5 | Copy, cut and paste between two editor tabs keep the text exactly | Pass | ⌘A ⌘C in the Russian tab, ⌘V, ⌘A ⌘X, ⌘V in a new tab: each step byte-identical to the file. |

## 8. Keyboard input and IME

Install the Russian layout and the built-in Japanese IME first. The
maintainer does not run the Japanese rows (K3–K7): mark them Unavailable
unless Claude observed them on macOS (ADR 0057).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| K1 | Russian typed into the editor and into the find field appears correctly | Pending | Physical keyboard. Injected Cyrillic reaches the Open and Save panels intact. |
| K2 | A misspelled Russian word gets the spelling underline; Mod+; and Mod+Shift+; move between misspellings | Pending | Physical keyboard. The underline shows on «Марищей». |
| K3 | Japanese in the editor: the marked text is underlined while composing | Unavailable | No Japanese input source; the maintainer does not run the Japanese rows (ADR 0057). |
| K4 | The candidate window opens at the caret, not at the window corner | Unavailable | As K3. |
| K5 | Choosing a candidate commits it; Esc during composition cancels it | Unavailable | As K3. |
| K6 | One undo removes the committed Japanese text | Unavailable | As K3. |
| K7 | Japanese in the find field composes, commits and finds the text | Unavailable | As K3. |
| K8 | *macOS*: Ctrl+⌘+Space opens the character palette and inserts an emoji | Pending | Physical keyboard. |
| K9 | Dead keys or Option/AltGr characters (e.g. `é`, `€`) type correctly | Pending | Physical keyboard. |

## 9. Shortcuts

Every application shortcut, from `bind_app_keys`, `bind_markdown_search_keys`,
`bind_shell_keys`, `pii::ui::bind_keys` and `mdoc_editor::bind_keys`.

| ID | Shortcut | Action | Result | Note |
| --- | --- | --- | --- | --- |
| H1 | Mod+N | New | Pass | New tab. |
| H2 | Mod+O | Open | Pass | Open dialog. |
| H3 | Mod+Shift+I | Convert to Markdown | Pending | ⇧ shortcut. |
| H4 | Mod+S / Mod+Shift+S | Save / Save As | Pass | ⌘S saved a new tab through the Save dialog. ⇧⌘S is Pending (Save As ran from the File menu, P2). |
| H5 | Mod+W | Close tab | Pass | Closes a saved tab at once; a dirty tab asks "Save changes to “Untitled.md”?"; a scanned one asks "Keep the replacement mapping?". |
| H6 | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab | Pass | From the editor ⌃Tab moves to the next tab and ⌃⇧Tab back (⌃⇧Tab injected; confirm by hand). |
| H7 | Mod+= , Mod+Shift+= , Mod+- , Mod+0 | Zoom in, in, out, reset | Pass | ⌘= 105 → 110 %, ⌘- 105 %, ⌘0 100 %. Injected ⇧⌘= zoomed in; Pending by hand. |
| H8 | Mod+, | Settings; Esc closes, Tab/Shift+Tab move between fields | Pass | Tab and ⇧Tab move with a visible ring (Details, English & Russian row); Esc closes. |
| H9 | Mod+K | Palette; ↑/↓, Enter, Esc | Pass | ↓/↑ move, Esc closes, "theme" + Enter ran Switch theme. |
| H10 | Mod+Shift+H | Home | Pending | ⇧ shortcut; injected ⇧⌘H went Home. |
| H11 | Mod+F, Mod+G, Mod+Shift+G, Enter, Shift+Enter, Esc | Find, next, previous, close | Pass | "the" 14/16; ⌘G 15, ↩ 16, ⇧↩ 15, ⇧⌘G 14; Esc closes. The ⇧ keys were injected; confirm by hand. |
| H12 | Mod+Shift+P | Pseudonymize | Pending | ⇧ shortcut; injected ⇧⌘P showed the setup card and later scanned. |
| H13 | Mod+B, Mod+I, Mod+E, Mod+U, Mod+Shift+X | Bold, italic, code, underline, strike | Pass | `**delta**`, `*ital*`, `` `code` ``, `<u>under</u>`, `~~strike~~` in the copied Markdown. ⇧⌘X was injected; Pending by hand. |
| H14 | Mod+A, Mod+C, Mod+X, Mod+V, Mod+Z, Mod+Shift+Z (Ctrl+Y on Windows) | Editing | Pass | ⌘A, ⌘C, ⌘X, ⌘V (C5); one ⌘Z restored a full-document deletion. ⇧⌘Z is Pending. |
| H15 | Alt+←/→, Alt+Shift+←/→, Home/End, Shift+arrows, Tab/Shift+Tab | Word moves, selection, indent | Pass | ⌥←/→, ⌥⇧← selects a word, Home goes to after the list marker, End, Tab nests a list item. ⇧ arrows and ⇧Tab are Pending. |
| H16 | Mod+Q | Quit, with the unsaved-changes prompt | Pass | ⌘Q asked "Save changes to “name”?" (Save, Discard, Cancel) for each of three dirty tabs; Discard on each quit. |
| H17 | *macOS*: every item in the mdoc, File, Edit, View and Window menus runs its action and shows its shortcut; Edit › Paste works in the find field and in a Save dialog (ADR 0058) | Pass | All five menus list their shortcuts (for example Paste AI Response ⌥⇧⌘V, Toggle Sidebar ⌃⌘S, Before / After ⌘\\). About, Save As…, Copy Markdown, Copy for AI, Close Tab, Fill and Toggle Sidebar ran; Edit › Paste filled the Save dialog's name field. Edit › Paste into the find field was not rechecked on the RC (it passed in the dry run). |
| H18 | F6 / Shift+F6 cycle sidebar, toolbar, editor, Replacements, Original and Comments (open ones only), returning to the last control in each; Tab / Shift+Tab move between controls with a visible focus ring and, at a region's last (first) control, on to the next (previous) region, so Tab past the toolbar enters the editor; in the sidebar ↑/↓ move between tabs, ⌫ closes one and Shift+F10 opens its case actions; the category list keeps focus until closed; Esc closes menus and returns from the sidebar or toolbar to the editor (ADR 0062) | Pass | Read from `AXFocused`. F6 goes editor → Replacements → Original → sidebar → toolbar → editor; ⇧F6 reverses. Tab past the toolbar's Share enters the editor area (notice, find bar, view switcher). Tab from Home reaches Open files…, Paste text, Restore AI reply, Pseudonymize clipboard, Open a case… and the Recents rows. In the sidebar ↑/↓ move, ⌫ closed `reference.pdf`, ⇧F10 opens the tab's menu (below), Esc returns to the row and a second Esc to the editor. The category list needs a saved case (Pending, below). |
| H19 | Enter, Shift+Enter in the editor | New paragraph, line break; Copy for AI pasted into a chat keeps both apart (ADR 0060) | Pass | Enter wrote a blank line, ⇧↩ a `\` line break, in the copied Markdown. Pasting into a chat is Pending (C1). |
| H20 | *macOS*: ⌘←/→ on a wrapped paragraph and a list item, ⌘↑/↓, each with ⇧ | Visual line start / end (after a list marker first), document start / end; ⇧ selects (ADR 0061) | Pass | Typed markers: ⌘← and ⌘→ stop at the start and end of the paragraph's third visual row; ⌘← on a list item lands after the marker; ⌘↑ / ⌘↓ reach the document's start and end. The ⇧ variants are Pending. |
| H21 | *macOS*: ⌥⌫, ⌥⌦, ⌘⌫ | Delete word left, word right, to the line start | Pass | Each, checked on screen and in the copied Markdown. |

### Keyboard reach follow-ups (ROADMAP 0.2.0 must-fix 1)

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| F6-explainer | F6 stays inside the Keychain explainer | Pass | F6 and ⇧F6 keep focus on the explainer's buttons. |
| Explainer Tab | Tab and ⇧Tab cycle the Keychain explainer's two buttons | Pass | Tab: Not now → Continue → Not now; ⇧Tab the reverse (`AXFocused`). The dry run's fail is fixed. |
| ⇧F10 | Shift+F10 opens a sidebar tab's case actions in both sidebar layouts | Pass | Expanded sidebar and collapsed rail: the menu ("Save mapping as case", "Close tab") opens, Tab moves inside it, Esc returns to the row. In the rail, focus shows the sidebar as an overlay. Case actions for a saved case are Pending with the next row. |
| Categories | "Categories to detect" takes focus, holds Tab and returns focus on Esc or Close | Pending | Needs a saved case, and saving one needs the Keychain password. |
| Recents names | VoiceOver reads each Home Recents row's name | Pass | Each row is a named button, for example "Мировое соглашение копия.md in Папка с пробелами, opened just now"; Tab reaches the rows and Return opens one. The dry run's fail is fixed. Spoken output is the maintainer's (A5 list). |

## 10. Case key storage

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| E1 | Nothing asks for the key at launch | Pass | No prompt at launch, on relaunch with restored tabs, or on opening documents; no SecurityAgent window. |
| E2 | *macOS*: Save mapping as case shows the "mdoc will ask for Keychain access" explainer, then the system dialog; Always Allow saves the case | Pending | The explainer appears on a folder drop, as a dialog with its title and a captioned drawing; no SecurityAgent window before Continue. The system dialog needs the Keychain password. |
| E3 | *macOS*: Not now (Esc) cancels the action, and the locked-cases bar offers Unlock… | Pass | Esc closed the explainer and created no case. The Unlock… bar needs locked cases (Pending). |
| E5 | After quit and relaunch, opening a case member restores its replacements without a prompt | Pending | Needs a saved case. |

## 11. Screen reader basics

VoiceOver (⌘F5) on macOS, Narrator (Ctrl+Win+Enter) on Windows. The editor's
text is not exposed to assistive technology (known limitation, ADR 0057); do
not mark it Fail.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| A1 | Sidebar toggle, tabs, toolbar buttons and the Pseudonymize control announce a name and role | Pass | Accessibility tree: Collapse sidebar, Search or run an action, Home, the "Open documents" tab group with named tabs and "Close <name>" buttons, Settings, "Theme: Light", Help, Hide replacements, Hide the original file, More actions, Share/Pseudonymize, the view switcher and zoom buttons, and Home's Recents rows are named. |
| A2 | Replacements panel controls and popup buttons announce a name and role | Pass | Panel rows ("Ms Bennett to PERSON_2, 6 mentions, 6 applied, 0 kept, expanded"), checkboxes, search and filter; popup "Replace · untick to keep 6 originals", Alias field, "Correct category for selected scope", "This one" / "All 6 mentions" toggles, Previous / Next mention and "Close replacement popup". |
| A3 | Settings and the Keychain/unsaved-changes dialogs announce their title and buttons; focus stays inside | Pass | The Keychain explainer and "Save changes to “Untitled.md”?" are AXDialogs with their titles and named buttons; Tab ×3 and F6 stay inside. Settings keeps Tab inside (H8). Spoken output is Pending. |
| A4 | Every control in A1–A3 can be reached and activated by keyboard alone (F6, Tab, arrows, Enter, Space), from Home and from the editor, without a screen reader or Full Keyboard Access (ADR 0062) | Pass | F6, Tab and arrows reach the sidebar, toolbar, panel, popup, dialogs and Home's Recents rows (H18 and follow-ups), including both explainer buttons. |
| A5 | Scan status and toasts are announced | Pending | Spoken output is the maintainer's. A rescan finished before a status element could be read from the tree. |

## Other observations

Not checklist failures; for the maintainer to triage.

- Home's Recents labels `Соглашение об урегулировании.docx` "pseudonymized"
  although it was never scanned (only the setup card, cancelled), as in the
  dry run.
- Settings › Text recognition says "English is supported now; more languages
  will follow." while it offers English & Russian.
- After clicking the toolbar's Apply N, ⌘Z did nothing until the editor was
  clicked (focus on the toolbar).
- In the collapsed rail, F6 lands on Search rather than on the tab that last
  had focus in the expanded sidebar.
- ⌘Q on a scanned, unsaved document asks only "Save changes…", while ⌘W also
  asks "Keep the replacement mapping?".
- The English demo's "Flat 3, 41 Larch Road, Bristol BS6 9ZY" and "born
  14 March 1988" are not proposed.
- The find bar's match counter ("14/16") and the popup's mention counter
  ("4/6") are still not in the accessibility tree.
