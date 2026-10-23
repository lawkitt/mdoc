# macOS platform acceptance, 0.2.0 dry run

The macOS half of the [2026-10-23 dry run](README.md), from the template
[`docs/platform-acceptance.md`](../../../platform-acceptance.md)
([ADR 0057](../../../adr/0057-native-platform-acceptance.md)). **This is a dry
run on current `main`, not the CI release candidate**: a local, ad-hoc signed
`just package` build, run before 0.2.0 has an RC to find failures early.
ADR 0057 keeps dry-run results out of the tree; this one is recorded at the
maintainer's request.

Claude ran it through computer use, reading state from the accessibility tree
(`AXUIElement`) and the pasteboard. Rows that need a physical keyboard
(section 8, ⇧ shortcuts), spoken VoiceOver output, the Keychain password or a
system setting are **Pending** for the maintainer; where an injected ⇧
shortcut happened to work it is noted, but it does not replace the keyboard
check. The connection ran at about 30 KB/s (a generic speed test agreed), so
the 638 MB model download never finished: setup rows that need it are
Unavailable, and the installed models were copied from the maintainer's
profile into the test profile for the popup, theme and clipboard rows.

## Header

| Field | Value |
| --- | --- |
| Date | 2026-10-23 |
| Run by | Claude (computer use, the accessibility tree via AXUIElement, pasteboard reads); maintainer rows Pending |
| mdoc version and artifact | Dry run, not an RC: `main` at `d98fd1f` (0.1.2 plus Unreleased), `just package` → `mdoc_0.1.2_arm64.dmg`, ad-hoc signed, not notarized (SHA-256 `13b35c3cedd8c34c3ff3b79f7237617723629eb1807cb9427d26355da046f150`) |
| Machine, CPU, RAM, GPU | Reference MacBook, Apple M4 (10-core GPU), 24 GB |
| OS and build | macOS 15.7.7 (24G720) |
| Displays and scale | Built-in Liquid Retina, 2880 × 1864 Retina; no external display |
| Input sources installed | U.S., ABC, Russian (no Japanese) |

## 1. Install and first launch

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| I1 | *macOS*: the `.dmg` opens; dragging to Applications and launching shows no Gatekeeper warning beyond the standard "downloaded from the Internet" prompt | Unavailable | Local build: the `.dmg` holds `mdoc.app` and an Applications link, but only the notarized CI artifact can show the Gatekeeper prompt. |
| I2 | *macOS*: `spctl -a -vv /Applications/mdoc.app` reports `Notarized Developer ID` | Unavailable | Ad-hoc signed local build; `spctl` rejects it, as expected. Needs the CI artifact. |
| I5 | First launch shows Home with no error, no crash report notice, and no Keychain or network prompt | Pass | Clean profile: Home, no error, crash notice or prompt; no SecurityAgent window in the window list. |
| I6 | About/Settings shows the expected version | Pass | mdoc › About mdoc opens Settings › Updates: "mdoc 0.1.2 is the latest version." |

## 2. Themes

Fixture: the English demo DOCX, with the Source preview open and aliases applied.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| T1 | Light: shell, sidebar, editor, Replacements panel, popup, dialogs and menus are legible, with no unthemed or invisible controls | Pass | Shell, sidebar, editor, panel, popup, Settings, the unsaved-changes dialog, the Keychain explainer and the menus are legible. |
| T2 | Dark: the same | Pass | The same in Dark; the preview pages follow the theme. |
| T3 | System: switching the OS appearance while mdoc is open updates every visible surface without a restart | Pending | OS appearance is the maintainer's setting. |
| T4 | Original page tone follows the theme; the override in the Original header works in both themes | Pass | Dark: "Show original paper" gives white pages, "Show dark pages" returns. Light shows original pages already. |
| T5 | Highlights (proposed, applied, search, spelling) stay distinguishable in both themes | Pass | Proposed (category tints), applied (blue box), search (yellow, current orange), spelling (red squiggle) and kept (dotted) are distinct in Light and Dark. |
| T6 | *macOS*: traffic lights sit on the unified title row | Pass | On the sidebar's top row. |

## 3. Display scaling

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| D1 | *macOS*: Retina display; text and icons are crisp | Pass | Crisp at 2× in zoomed captures. |
| D2 | *macOS*: a 1× external display; text is legible and the window moves between displays without layout breakage (Unavailable without such a monitor) | Unavailable | No 1× external display at hand. |
| D5 | Resizing to the minimum window size keeps every control reachable | Pass | Minimum is 720 × 480; header, sidebar and editor controls stay visible, the tab list clips and scrolls. |

## 4. Files and paths

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| P1 | Open (Mod+O) `Соглашение об урегулировании.docx` from `Документы/Папка с пробелами/`; it converts and its preview shows | Pass | ⌘O; converted, Source preview shown, RU spelling underline on «Маришей». |
| P2 | Save As (Mod+Shift+S) into the same folder under a Cyrillic name with spaces; reopening it from Recents works | Pass | File › Save As… wrote `Мировое соглашение копия.md`; the tab badge became MD; Home Recents lists it and reopens it. ⇧⌘S itself is Pending. |
| P3 | Dropping that file onto Home opens it | Pass | Opens in a new tab; mdoc comes to the front. |
| P4 | Dropping two files onto the sidebar shows the insertion line and opens both at that position | Pass | `reference.pdf` and `legal-opinion.docx` between two tabs: insertion line, both opened there. |
| P5 | Dragging an unsupported file over Home or the sidebar says "These files can't be opened" (no insertion line) and opens nothing | Pass | `образец.xyz` over the sidebar and over Home says "These files can't be opened", no insertion line; the drop opens nothing and adds the notice "Unsupported files skipped: образец.xyz", which Esc closes. |
| P6 | Dropping a folder opens it as a case | Pass | Dropping `Папка с пробелами` opened its documents and showed the Keychain explainer; Esc cancelled. Creating the case needs the Keychain password (Pending with E2). |
| P7 | Opening a file from the OS (Finder/Explorer "Open with") opens it in mdoc | Pass | `open -a mdoc russian.pdf` opened it in a new tab. |

## 5. First-use model setup

Run on the clean profile.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| S1 | Offline (network off): Pseudonymize shows the consent card; Download & continue fails cleanly with Retry and no crash | Pending | Turning the network off is the maintainer's. A network drop mid-download showed "Couldn't reach huggingface.co. Check your internet connection, then Retry." with Retry and no crash. |
| S2 | Online: Retry downloads with byte progress, then Verifying / Checking runtime, and the scan resumes in the requesting tab | Unavailable | Byte progress shows (15 %, 104 / 665 MB) while working in other tabs, but at about 30 KB/s the download dropped twice and never finished. A failed file restarts from zero ("638 MB left"). |
| S3 | Cancel during download returns to the card; a second attempt completes | Unavailable | Cancel at 1 MB returned to the card with "Download cancelled." and Retry. The second attempt did not complete (network, as S2). |
| S4 | Converting `reference.pdf` with OCR shows the OCR consent card and completes after the download | Unavailable | `reference.pdf` has a text layer, so `tests/fixtures/ocr-qualification/russian.pdf` was used: the card warns the model reads English; Choose model › English & Russian. Its download did not finish (network); while the pseudonymization download ran it said "Another model job is running. Retry when it finishes." |
| S5 | Settings shows both models as installed after setup | Unavailable | Setup never completed; with the models copied in, Settings shows Standard pseudonymization as Installed. |

## 6. Replacement popup and panel

Fixture: the English demo, scanned (14 identities, 24 to replace).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| R1 | Clicking a proposal opens the popup next to it, inside the window, with buttons inside the popup | Pass | Below the mention, inside the window, every control inside the card. |
| R2 | Enter applies in the shown segment (All N, then This one) without changing it and opens the next proposal; ⌫ keeps, ←/→ step through mentions, Mod+↑/↓ jump to the ends, Esc closes | Fail | Enter with All 6 applied all six and moved to the next proposal; ⌫ keeps; ⌘↓ / ⌘↑ jump to 6/6 and 1/6; Esc closes. But when the next mention or proposal is already on screen (→ from 1/6 to 2/6, Enter moving on to "Mia"), the editor scrolls to the end of the document and the popup is off-screen. [Replacement popup scrolls the editor to the end when the next mention is already visible](https://github.com/lawkitt/mdoc/issues/102). |
| R3 | Mod+⌫ keeps the same text; Mod+Shift+⌫ keeps the whole entity | Pass | ⌘⌫ kept all 4 "Harrow & Finch Ltd" mentions. ⇧⌘⌫ is Pending. |
| R4 | Editing the alias field and pressing Enter renames the alias | Pass | ORG_2 → EMPLOYER; the panel follows. |
| R5 | Mod+Z inside the popup and the panel undoes the last decision; Mod+Shift+Z (and Ctrl+Y on Windows) redoes | Pass | ⌘Z in the popup undid the rename, then the keep; ⌘Z in the panel undid a keep. ⇧⌘Z is Pending. |
| R6 | Panel list: ↑/↓, Space, Enter, →/←, ⌫ behave as described in ADR 0048 | Pass | From the search, ↓ enters the list; ↓/↑ move with a visible ring, Space toggles keep, ← collapses, ⌫ keeps the entity, Enter reveals the mention in the editor. |
| R7 | Mod+Enter and the toolbar's Apply N apply all and close the popup; one Mod+Z undoes them | Pass | ⌘↩ with the popup open applied 23 (1 kept), closed it and switched to Share; one ⌘Z undid all; the toolbar's Apply 17 applied all too. |
| R8 | Before / After (Mod+\\) shows both columns; Esc leaves | Pass | Original and Pseudonymized side by side; Esc leaves. |
| R9 | Alt+Enter, Alt+↑/↓ and Mod+Alt+P work in the editor | Pass | ⌥↩ opens the popup at the caret; ⌥↓ moves to another proposal; ⌘⌥P on a selected word made a new proposal (PERSON_7, Person). |

## 7. Clipboard

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| C1 | Copy for AI, pasted into the ChatGPT and Claude web apps, arrives as plain text with the placeholder note | Pending | The pasteboard holds plain text only (no HTML/RTF), starts with the placeholder note and has no original names. Pasting into the web apps needs a browser: the maintainer's. |
| C2 | Copy Markdown of the Russian demo, pasted into TextEdit (plain text mode) or Notepad, is byte-identical to the saved file, Cyrillic intact; the same for a line with CJK text | Pass | Copy Markdown of `Соглашение об урегулировании.md` equals the file (3,844 bytes); a saved file with 契約 テスト 合意 equals its copy (3,868 bytes). Compared with `pbpaste`, not through TextEdit. |
| C3 | Paste AI Response (Mod+Alt+Shift+V) of a browser selection carrying HTML and plain text uses the plain text and restores aliases | Pass | A pasteboard with HTML (bold, red span) and plain text, via File › Paste AI Response: "Summary. Maya Bennett may settle with Harrow & Finch Ltd; Ms Bennett keeps the reference." ⌥⇧⌘V is Pending. |
| C4 | Pseudonymize Clipboard (Mod+Alt+Shift+C) replaces the known case names in the clipboard; pasting elsewhere shows the aliases | Pass | Edit › Pseudonymize Clipboard turned "Maya Bennett emailed Harrow & Finch Ltd…" into "PERSON_1 emailed ORG_1…" (names from the open document; no saved case). ⌥⇧⌘C is Pending. |
| C5 | Copy, cut and paste between two editor tabs keep the text exactly | Pass | ⌘A ⌘C in the Russian tab, ⌘V, ⌘X, ⌘V in a new tab: byte-identical. |

## 8. Keyboard input and IME

Install the Russian layout and the built-in Japanese IME first. The
maintainer does not run the Japanese rows (K3–K7): mark them Unavailable
unless Claude observed them on macOS (ADR 0057).

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| K1 | Russian typed into the editor and into the find field appears correctly | Pending | Physical keyboard. Injected Cyrillic reaches the find field and the Save dialog intact. |
| K2 | A misspelled Russian word gets the spelling underline; Mod+; and Mod+Shift+; move between misspellings | Pending | Physical keyboard. The underline shows on «Маришей». |
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
| H5 | Mod+W | Close tab | Pass | Closes the tab; on a dirty tab asks first. |
| H6 | Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab | Pass | ⌃Tab and ⌃⇧Tab move and wrap (⌃⇧Tab injected; confirm by hand). |
| H7 | Mod+= , Mod+Shift+= , Mod+- , Mod+0 | Zoom in, in, out, reset | Pass | ⌘= twice → 110 %, ⌘- → 105 %, ⌘0 → 100 %. ⇧⌘= is Pending. |
| H8 | Mod+, | Settings; Esc closes, Tab/Shift+Tab move between fields | Pass | Settings is a dialog titled "Settings"; Tab and ⇧Tab move with a visible focus, F6 stays inside, Esc closes. |
| H9 | Mod+K | Palette; ↑/↓, Enter, Esc | Pass | ↓ moves, Enter runs (Switch theme), Esc closes. |
| H10 | Mod+Shift+H | Home | Pending | ⇧ shortcut. |
| H11 | Mod+F, Mod+G, Mod+Shift+G, Enter, Shift+Enter, Esc | Find, next, previous, close | Pass | Russian query 2/7; ⌘G 3/7, ↩ 4/7, ⇧↩ 3/7, ⇧⌘G 2/7; Esc closes. The ⇧ keys were injected; confirm by hand. ⌥⌘F opens Find and Replace. |
| H12 | Mod+Shift+P | Pseudonymize | Pending | ⇧ shortcut; the toolbar's Pseudonymize scans. |
| H13 | Mod+B, Mod+I, Mod+E, Mod+U, Mod+Shift+X | Bold, italic, code, underline, strike | Pass | `**delta**`, `*ital*`, `` `code` ``, `<u>under</u>` in the copied Markdown. ⇧⌘X is Pending. |
| H14 | Mod+A, Mod+C, Mod+X, Mod+V, Mod+Z, Mod+Shift+Z (Ctrl+Y on Windows) | Editing | Pass | ⌘A, ⌘C, ⌘X, ⌘V; one ⌘Z restored a full-document deletion. ⇧⌘Z is Pending. |
| H15 | Alt+←/→, Alt+Shift+←/→, Home/End, Shift+arrows, Tab/Shift+Tab | Word moves, selection, indent | Pass | ⌥←/→, ⌥⇧← (selection), Home to after the list marker, End, Tab indents a list item. ⇧ arrows and ⇧Tab are Pending. |
| H16 | Mod+Q | Quit, with the unsaved-changes prompt | Pass | ⌘Q asks "Save changes to “name”?" per dirty tab (Save, Discard, Cancel); Discard on each quit. Separately, ⌘Q does nothing while a model downloads from its card: [⌘Q and Quit do nothing while a model downloads from its setup card](https://github.com/lawkitt/mdoc/issues/103). |
| H17 | *macOS*: every item in the mdoc, File, Edit, View and Window menus runs its action and shows its shortcut; Edit › Paste works in the find field and in a Save dialog (ADR 0058) | Pass | All five menus show their shortcuts (Toggle Sidebar ⌃⌘S, Find and Replace… ⌥⌘F). About, Save As…, Copy Markdown, Paste AI Response, Pseudonymize Clipboard, Toggle Sidebar, Next / Previous Tab, Find… and Fill ran. Edit › Paste fills the find field; ⌘V pastes a path into the Save dialog's Go to Folder. |
| H18 | F6 / Shift+F6 cycle sidebar, toolbar, editor, Replacements, Original and Comments (open ones only), returning to the last control in each; Tab / Shift+Tab move between controls with a visible focus ring; in the sidebar ↑/↓ move between tabs, ⌫ closes one and Shift+F10 opens its case actions; the category list keeps focus until closed; Esc closes menus and returns from the sidebar or toolbar to the editor (ADR 0062) | Fail | F6 goes sidebar → toolbar → editor → Replacements → Original and wraps, ⇧F6 reverses, each landing on the last control. Tab from Home reaches Open files…, Paste text, Restore AI reply, Pseudonymize clipboard, Open a case… and the sidebar, and wraps. In the sidebar ↑/↓ and ⌫ work; ⇧F10 opens the tab's menu in the expanded sidebar and the collapsed rail, Tab stays in it, Esc returns to the row, and a second Esc to the editor. Fails: Home's Recents rows are not Tab stops ([Later: Home's Recents rows](../../../../ROADMAP.md#later)), and in the Keychain explainer Tab cannot return to Continue once it leaves ([Later: Keychain explainer](../../../../ROADMAP.md#later)). Case actions and the category list need a saved case (Pending, below). |
| H19 | Enter, Shift+Enter in the editor | New paragraph, line break; Copy for AI pasted into a chat keeps both apart (ADR 0060) | Pass | Enter wrote a blank line, ⇧↩ a `\` line break, in the copied Markdown. Pasting into a chat is Pending (C1). |
| H20 | *macOS*: ⌘←/→ on a wrapped paragraph and a list item, ⌘↑/↓, each with ⇧ | Visual line start / end (after a list marker first), document start / end; ⇧ selects (ADR 0061) | Pass | ⌘→ / ⌘← stop at the visual row's end and start, after the list marker on a list item; ⌘↑ / ⌘↓ go to the document's start and end. The ⇧ variants are Pending. |
| H21 | *macOS*: ⌥⌫, ⌥⌦, ⌘⌫ | Delete word left, word right, to the line start | Pass | Each, checked in the copied Markdown. |

### Keyboard reach follow-ups (ROADMAP 0.2.0 must-fix 1)

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| F6-explainer | F6 stays inside the Keychain explainer | Pass | F6 and ⇧F6 keep focus on the explainer's buttons. Tab inside it fails (H18). |
| ⇧F10 | Shift+F10 opens a sidebar tab's case actions in both sidebar layouts | Pass | Opens the tab's menu in the expanded sidebar and the collapsed rail, takes Tab, Esc returns. Without a saved case the menu holds Close tab only; case actions are Pending with the next row. |
| Categories | "Categories to detect" takes focus, holds Tab and returns focus on Esc or Close | Pending | Needs a saved case, and saving one needs the Keychain password. |

## 10. Case key storage

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| E1 | Nothing asks for the key at launch | Pass | No prompt at launch or on opening documents; no SecurityAgent window. |
| E2 | *macOS*: Save mapping as case shows the "mdoc will ask for Keychain access" explainer, then the system dialog; Always Allow saves the case | Pending | The explainer appears on a folder drop, with a dialog role and title; no SecurityAgent window before Continue. The system dialog needs the Keychain password. |
| E3 | *macOS*: Not now (Esc) cancels the action, and the locked-cases bar offers Unlock… | Pass | Esc closed the explainer and created no case. The Unlock… bar needs locked cases (Pending). |
| E5 | After quit and relaunch, opening a case member restores its replacements without a prompt | Pending | Needs a saved case. |

## 11. Screen reader basics

VoiceOver (⌘F5) on macOS, Narrator (Ctrl+Win+Enter) on Windows. The editor's
text is not exposed to assistive technology (known limitation, ADR 0057); do
not mark it Fail.

| ID | Check | Result | Note |
| --- | --- | --- | --- |
| A1 | Sidebar toggle, tabs, toolbar buttons and the Pseudonymize control announce a name and role | Pass | Accessibility tree: sidebar toggle, search, Home, the "Open documents" tab group with named tabs and "Close <name>" buttons, Settings, "Theme: Dark", Help, More actions, Pseudonymize and the zoom group are named. Home's Recents rows are missing (A4). |
| A2 | Replacements panel controls and popup buttons announce a name and role | Pass | Panel rows ("Ms Bennett to PERSON_2, 6 mentions, 6 applied, 0 kept, expanded"), checkboxes and search; popup Replace checkbox, Alias field, category button, "This one" / "All 6 mentions" toggles, Previous / Next mention and Close are named. |
| A3 | Settings and the Keychain/unsaved-changes dialogs announce their title and buttons; focus stays inside | Pass | Each is an AXDialog with its title ("Settings", "mdoc will ask for Keychain access", "Save changes to “Untitled.md”?") and named buttons; Tab and F6 stay inside. Spoken output is Pending. |
| A4 | Every control in A1–A3 can be reached and activated by keyboard alone (F6, Tab, arrows, Enter, Space), from Home and from the editor, without a screen reader or Full Keyboard Access (ADR 0062) | Fail | F6, Tab and arrows reach the sidebar, toolbar, panel, popup and dialogs (H18), except Home's Recents rows and, once left, the Keychain explainer's Continue. [Later: Home's Recents rows](../../../../ROADMAP.md#later); [Later: Keychain explainer](../../../../ROADMAP.md#later). |
| A5 | Scan status and toasts are announced | Pending | Spoken output is the maintainer's. Download progress is an AXApplicationStatus group ("Downloading 0%"). |

## Other observations

Not checklist failures; for the maintainer to triage.

- The find bar's match counter ("2/7") and the popup's mention counter ("1/6")
  are not in the accessibility tree.
- Review and Share in the panel report Value 0 for both, so the selected tab
  is not exposed; a panel list row with the focus ring is not the AX focused
  element.
- Home's Recents labels a file "pseudonymized" as soon as review starts, before
  any scan or decision (`DocumentView::pseudonymized` counts `reviewing`).
- A model download that fails mid-file discards the partial file and restarts
  it ("638 MB left" after about 200 MB had arrived).
