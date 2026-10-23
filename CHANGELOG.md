# Changelog

All notable changes to **mdoc** are documented here.

## Unreleased

## [0.1.5] - 2026-10-23

- **Tab moves between regions.** Tab past the toolbar now enters the editor
  instead of skipping it, and Tab from the Replacements panel's last control
  moves on to the next region instead of looping inside the panel. ⇧Tab from
  the panel returns to the editor. F6 is unchanged, and the editor still
  takes Tab for Indent.

- **Home's Recents and Cases rows work by keyboard.** Each row is a Tab stop
  with a name for screen readers, and Enter or Space opens it.

- **Keychain explainer keeps Continue reachable.** Tab and ⇧Tab now move
  between Not now and Continue instead of sticking on Not now. The OCR and
  pseudonymization setup cards' buttons had the same fault and are fixed too.

- **Expand or collapse all identities.** A toggle beside the Replacements
  panel's filter, and two palette commands, open or close every group.

- **Go to heading.** The command palette lists the document's headings;
  choosing one moves the caret there and scrolls it into view.

- **Contrast checks.** Tests hold text to WCAG 4.5:1 and controls to 3:1 in
  both themes. Light muted text, light applied aliases, dark destructive menu
  rows, category-coloured mention text and the notices badge are adjusted to
  pass, and nothing is drawn below 10 px.

## [0.1.4] - 2026-10-23

- **First published release with the 0.1.3 changes.** The 0.1.3 release build
  stopped at a stale OCR smoke-test check and was never published; 0.1.4 ships
  the same changes with the test fixed.

## [0.1.3] - 2026-10-23

- **Dependency security update.** `ctutils` 0.4.5 fixes non-constant-time
  comparisons (RUSTSEC-2026-0385) in the crypto crates behind case encryption.

- **Replacement navigation keeps visible mentions in place.** Stepping through
  mentions or applying a proposal with Enter no longer scrolls away from an
  already-visible target. Off-screen mentions scroll just far enough to show
  the mention and its popup.

- **Quit works during model setup.** Quit and app-level commands stay available
  without keyboard focus. Both the pseudonymization and OCR cards keep focus on
  Cancel when Download becomes progress. Quit still asks about unsaved documents
  and cancels setup after those prompts are resolved (fixes #103).

- **Reliable PDF scrolling tests on Windows.** The tests carry their PDF fixture
  with them, so cached binaries still work after a checkout moves. They wait for
  the PDF to load and report load errors before checking toolbar and scroll bounds.

- **DOCX previews keep words apart.** The local renderer now resolves theme
  fonts correctly, preserving word spacing in bold justified headings with tabs.

- **Offscreen screen evidence on macOS.** `just headless-renders <directory>`
  writes light/dark PNGs for Home, a document, Review, Settings and app dialogs,
  plus actual Tab walks, focus-ring images and frame costs in JSON. Dev-only,
  with a temporary profile and no visible window or activation. Windows's
  headless DirectX adapter remains a follow-up.

- **Pick the recognition languages on the OCR card.** The card offers English
  or English & Russian for this document and picks a default: English &
  Russian when the PDF's text, its file name (Cyrillic letters, or a word
  such as "russian" or "ru") or, failing those, a Russian keyboard suggests
  Russian; otherwise your Settings choice. One line says why, for example
  "English & Russian: the file name is in Russian" (ADR 0066).

- **Small polish.** View › Toggle Sidebar has ⌃⌘S on macOS. Two tabs of the
  same name in one folder (a DOCX's conversion beside an `.md`) show their
  type, not the full path. The DOCX notice about equations, charts and
  SmartArt shows only when the file has them. A search match at the start of
  a wrapped line, or wrapping at a space, no longer paints a block at the end
  of the line above. After Save As the tab's badge says MD; its tooltip names
  the source. The window title is "mdoc" while Home shows. A drop from
  Finder brings mdoc to the front, so a dialog it opens gets the keyboard.
  Unsupported files dragged over the sidebar show "These files can't be
  opened" instead of the insertion line and open nothing; a mixed drop's
  notice names skipped files without their folders, and Esc closes it. The
  source preview's Find button no longer claims ⌘F (that finds in the
  Markdown). The theme button's tooltip follows each click. A setup cancelled
  part-way says "MB left to download".

## [0.1.2] - 2026-10-22

- **Dialogs and choices read correctly with a screen reader.** Settings is
  announced as a dialog; model choices as radio buttons in a named group; the
  narrow window's Markdown / Source switch as a "View" choice; each tab's
  close button names its tab. Every confirmation (unsaved changes, model
  removal, case actions) is now mdoc's own dialog, named by its question
  rather than "Warning": unsaved changes ask "Save changes to “name”?" with
  Save, Discard (⌘D / Alt+D) and Cancel (Esc) (ADR 0064).

- **Find and Replace.** ⌘⌥F / Ctrl+H (or Edit ▸ Find and Replace…) opens find
  with a Replace row. Enter replaces the current match and moves on; ⌘↩ /
  Ctrl+Alt+Enter replaces all of them as one undo step. Whole word joins
  Match case (⌥⌘W / Alt+W and ⌥⌘C / Alt+C). Replace leaves applied aliases
  and matches that cross hidden formatting alone and says how many it skipped
  (ADR 0063).

- **Every control is reachable by keyboard.** F6 and Shift+F6 move between the
  sidebar, the toolbar, the document, the Replacements panel and the original,
  returning to the control you left in each. Tab from Home reaches its
  buttons. In the sidebar, ↑ and ↓ move between documents and Backspace closes
  one; Shift+F10 opens its case actions. Esc in the sidebar or toolbar returns
  to the document. Case category choices and the Keychain explainer keep focus
  inside until closed (ADR 0062).

- **macOS caret keys.** ⌘← and ⌘→ go to the start and end of the line as
  you see it wrapped (after a list bullet first), ⌘↑ and ⌘↓ to the start and
  end of the document, and Shift with each selects. ⌥⌫ and ⌥⌦ delete a word,
  ⌘⌫ deletes to the line start (ADR 0061).

- **Enter in the mention popup follows the scope you see.** With All N
  selected, Enter replaces every mention of the name; with This one, only
  this mention. The choice no longer flips to This one, and the popup moves
  on to the next suggestion, closing when none is left. Apply all (⌘↩ /
  Ctrl+Enter, or the toolbar's Apply) closes the popup (ADR 0055).

- **Enter starts a paragraph.** Lines you separate with Enter stay separate
  in Copy for AI: Enter writes a blank line between paragraphs, and Shift+Enter
  is a line break within one (`\` at the line end, hidden away from the
  caret). Lists, quotes, tables and code keep their own Enter; leaving a list,
  quote or table leaves a blank line. Backspace at a paragraph's start joins
  it to the one above in one press. Existing documents are not changed
  (ADR 0060).
- **An OCR run that finds no text stays with its PDF.** Other PDF tabs, and
  PDFs opened afterwards, offer Run OCR as usual instead of showing that
  file's error and Retry.
- **Esc closes the Replacements panel's right-click menu** first; with the
  mention popup open, a second Esc closes the popup.
- **A failed conversion points to the notice below**, where it is shown,
  instead of "above".

## [0.1.1] - 2026-10-22

- **Shortcuts keep working after Save as case and after undoing in the
  popup.** After Share › Save as case, focus stays in the Share panel, so
  Ctrl+Q / ⌘Q and other shortcuts work without clicking the document first. An undo that
  closes the mention popup returns focus where Esc would, so redo still works.
- **Bold and other formatting stay valid with a trailing space.** Formatting a
  selection that starts or ends with a space keeps the space outside the
  markers (`**delta** `), so Copy for AI keeps the formatting.
- **A converted document saved as Markdown enters Recents** as the saved
  file, so it reopens as your edited Markdown instead of a fresh conversion of
  the source. On macOS, a newly saved file gets the usual permissions instead
  of being readable only by you.
- **Save as case keeps the document.** On a conversion whose Markdown was
  never saved, Save as case (or Save as Case when closing) first asks where to
  save it, then creates the case with that file in it. It no longer creates
  an empty case that can't be opened.
- **One mdoc on Windows.** A second launch or **Open with** hands its files to
  the running mdoc, which opens them and comes to the front, instead of
  starting another copy that also writes the session and Recents.
- **mdoc is offered under Open with on Windows** for Markdown, text, PDF and
  Word files, never as the default app; uninstalling removes exactly those
  entries.
- **Windows word and document moves.** Ctrl+←/→ (with Shift) move by word and
  stop at the next word's start, Ctrl+Backspace/Delete delete a word, and
  Ctrl+Home/End (with Shift) go to the document's ends without revealing
  hidden syntax.
- **The command palette covers what the Windows menu lacks:** Save As,
  Before / After, Close Tab and Toggle Sidebar. Settings › Saved cases now
  points to Home and the palette.

## [0.1.0] - 2026-10-20

- **Windows window buttons work.** Minimize, maximize/restore and close act on
  a click, hovering maximize shows snap layouts, and double-clicking the title
  bar toggles maximize and restore (ADR 0048).
- **Standard macOS menus.** The menu bar now has mdoc, File, Edit, View and
  Window menus: About, Hide (⌘H), Services, menu-bar Undo, Cut, Copy and Paste
  (which also work in Open and Save dialogs), Find, zoom, Minimize (⌘M) and tab
  switching (ADR 0058).
- **The Keychain explainer's picture can't be mistaken for the real dialog.**
  The animated preview of the macOS password dialog now sits below the
  explanation, captioned "After Continue, macOS shows this:". macOS still asks
  only after you press Continue (ADR 0043).
- **The document follows your selection.** Dragging a selection to the top or
  bottom edge scrolls the page, even while the pointer stays still, and
  Shift+arrow selection, arrow keys and typing keep the caret in view with a
  few lines of context (ADR 0054).
- **The zoom and view pill no longer covers text.** It fades after a moment
  and comes back when you move the pointer near the bottom of the document,
  scroll, zoom or switch views. It stays while the pointer is on it, and
  ⌘= / ⌘− still zoom (ADR 0048).
- **Word previews say they are approximate**, with an ⓘ beside the file name
  instead of a notice. The preview header now reads **Source · name**
  (ADR 0048).
- **Crash reports you check first.** If mdoc hits an internal error, the next
  launch says so and offers **See what would be sent**: the report with folder
  names and quoted text masked, and never document text. **Send…** opens a
  prefilled GitHub issue for you to review; **Copy** and **Dismiss** also
  work. Nothing is sent automatically (ADR 0047).
- **View switcher.** While replacements are applied, a floating control at the
  bottom of the document switches between **Edit**, **Original**,
  **Pseudonymized** and **Side by side**. The toolbar button and ⌘\ still
  toggle Side by side (ADR 0046).
- **The empty page says documents stay on this Mac**, with a hover note on the
  only network use: the update check and model downloads you choose. Long
  tooltips now wrap (ADR 0046).
- **Replacements panel groups by category.** Names are listed under a label
  per category with counts, e.g. "Person 3 (7)": names, then mentions. The
  labels are not selectable; arrow keys skip them (ADR 0046).
- **Paste AI Response** with a reply that has no placeholders now says
  "No placeholders found in this reply." instead of "Restored 0 mentions of
  0 values" (ADR 0046).
- **Line breaks in table cells.** `<br>` inside a table cell, as converted
  Word tables produce, now shows as a line break instead of literal text. The
  saved Markdown is unchanged (ADR 0046).
- **Categories to detect** keeps its close button inside the popup for long
  case names; the name is shortened under the title. Long names in
  **Move to case “…”** are shortened too. The panel footer no longer reads
  "not in this document (0)": with nothing to list it offers
  **Add a name to “case”** (ADR 0046).
- **No Keychain dialogs at launch.** When a saved-case key was made by another
  build, launching mdoc could still show two macOS password dialogs: the quiet
  check reported "denied" where macOS meant "would ask", and launch then read the
  key with dialogs allowed. Launch now leaves such cases locked behind
  **Unlock…**. Builds signed with a provisioning profile keep the key in the data
  protection keychain, which never asks for the login password. An existing key
  moves there on the next Unlock, after one last dialog. Development builds keep
  their own key and cases folder (ADR 0045).
- **Keychain access is explained before macOS asks.** mdoc no longer reads the
  saved-case key at launch while doing so could show the macOS password dialog.
  Until it can, saved cases stay locked behind an **Unlock…** bar. Saving a case,
  opening one, or unlocking first shows a short explainer with an animated
  preview of the macOS dialog, recommending **Always Allow**. Declining keeps
  the documents linked to their case and offers **Try again…** (ADR 0043).
- **Replacements panel:** Apply, Keep and Undo leave the selection on the
  mention you acted on, rather than jumping to the next one; after Keep, its
  group header is selected. Applied mentions show a blue bar, dimmed when not
  selected. Group headers lose their ▸/▾ markers and line up with their
  mentions. A mention row repeats the original only when its wording differs.
  A selected row shows one Undo. The panel's close button now matches the
  Original viewer's (ADR 0044).
- **Before / after** follows the Replacements panel live: applying, undoing or
  keeping originals updates both columns with the editor's replacement motion.
  The header row is gone: quiet floating labels name the columns, a floating
  **Close** (or Esc) leaves the view, and originals are marked gold beside blue
  aliases. The toolbar shows a two-column icon that stays pressed while the view
  is open (⌘\\, also in the File menu). Selecting a replacement scrolls both
  columns to it, clicking a name selects its row, and narrow windows show one
  column at a time. Opening the view hides the PDF preview until it closes
  (ADR 0042).
- Dropping a folder opens its files (not subfolders) as one new case named after
  it; each document joins once it has Markdown. **File → Pseudonymize
  Clipboard** (⌥⇧⌘C) replaces names the active tab's case knows in the
  clipboard text, without running detection. **Before / after** shows the
  original wording beside the pseudonymized text, line by line. A case's
  **Categories to detect…** switches categories off for scans in its documents
  (ADR 0041, phase 4).
- Cases with several documents: right-click a tab and choose **Move to case**,
  or drag it onto a case's header. Names the case already knows take its
  aliases, other names that clash get the next free number (one undo step), and
  the case's other names are proposed wherever they appear. Renaming an alias or
  changing its category in one document updates the other open documents of the
  case, each as its own undoable edit. The replacements panel lists the case's
  names this document doesn't mention and adds new names to the case before they
  appear. Case headers fold their documents away (ADR 0041, phase 3).
- Saved cases: right-click a tab with applied replacements and choose **Save
  mapping as case** to keep which original each alias stands for, encrypted on
  this computer (AES-256-GCM; the key is held by the macOS Keychain, Windows
  data protection or, on Linux, the Secret Service keyring such as GNOME
  Keyring or KWallet). The sidebar shows the case above its document; later
  mapping changes save automatically. Reopening the document gives its aliases
  back their originals, so they can be restored and AI replies pasted again. Old
  aliases keep restoring after a rename. **File → Open Case…** lists saved
  cases; a case can be renamed, a document removed from it, or the case deleted
  and its originals forgotten. Closing a tab whose mapping isn't saved asks
  once (Settings → Saved cases turns this off). On Linux without a keyring,
  cases are unavailable (ADR 0041, phase 2).
- AI return journey: after applying replacements, **Copy for AI** copies the
  Markdown with a short note asking the assistant to keep placeholders such as
  `PERSON_1` exactly. **Paste AI Response** (⌥⇧⌘V, the toolbar or the File
  menu) opens the assistant's reply in a new tab with the original names
  restored as proposals that keep their aliases, so Apply turns them back into
  aliases before the next round. Reshaped aliases (`person 1`, `PERSON-1`,
  `**PERSON_1**'s`, `PERSON_1-у`) restore; placeholders the original document
  never had are highlighted in red and can be assigned to a name or left as
  they are. Nothing is saved to disk (ADR 0041, phase 1).
- In-app updates on macOS: Settings → Updates offers **Download and install**
  for a newer release, then **Restart now**. The download is checked against
  the release's `SHA256SUMS` and must carry the mdoc developer's Apple
  signature, pass Gatekeeper and contain the promised version; the installed
  app is replaced only while mdoc quits, and kept if anything fails. A banner
  at the top of the window shows an available update, download progress, a
  ready update and failures, and opens Settings → Updates. Nothing downloads
  without a click; copies that can't replace themselves (not in Applications,
  running from the disk image, unsigned builds) keep **View release**
  (ADR 0040).
- macOS releases are signed with a Developer ID and notarized by Apple, so mdoc
  opens normally on first launch; the release gate checks Gatekeeper accepts the
  `.dmg` and the app (ADR 0039).
- Failed model downloads say plainly that GitHub couldn't be reached, or name
  the proxy or HTTP problem, instead of showing the raw network error.
- Update check: at launch mdoc asks GitHub whether a newer release exists and
  marks the Settings button with a dot when one does. Settings → Updates shows
  the version and notes with **View release**, **Check now**, and switches for
  the launch check and pre-releases (always on in pre-release builds). It only
  sends the app version and never downloads or installs (ADR 0038).
- Releases: Apple Silicon `.dmg` and Windows x64 installer (`.msi` for stable
  versions), with `SHA256SUMS` and build provenance. The Windows installer
  installs the Visual C++ Redistributable that OCR needs when it is missing
  (ADR 0037).

- Dark Original pages: in the dark theme PDF and DOCX pages render on dark
  paper with light ink, keeping the colour of coloured text. The ◐ button in
  the Original header switches a document back to the original paper (ADR 0035).

- Replacements panel: Undo, Redo and Rescan icon buttons in its toolbar, and
  ⌘Z / ⇧⌘Z work while the panel has focus. Scan progress and Cancel live only
  in the main toolbar; the ⋯ menu is gone (Model settings are in Settings).
  After Apply, Keep or Replace a short "… · Undo" notice appears. Pseudonymization
  is no longer labelled experimental (ADRs 0033, 0034).
- Replacements panel groups open independently (a header click only toggles),
  with grey rows, Expand/Collapse all, an All/Proposed/Applied filter, and the
  alias and category editable on the group header.
- Selecting a mention in the panel shows inline Apply/Keep controls instead of
  the popup. Keys in the panel and the popup: Enter / ⌘Enter / ⇧⌘Enter apply
  this one / same text / all; Delete with the same modifiers keeps originals.
  ↑/↓ move through the list and focus follows to the next undecided mention.
- ⌥↑/↓ moves a mention or group to another entity (release ⌥ to drop, Esc to
  cancel). A mouse wheel steps through the list; a middle click keeps an
  original, in the panel and on editor chips.

- Removed interactions that did nothing in mdoc (ADR 0031): the PDF
  toolbar's Highlight (⌘⇧H) and Area highlight tools; the caret now enters
  `$…$` and `$$…$$` math like other Markdown, Backspace/Delete edit it
  character by character, and right-click shows the normal menu; clicking
  an inline image places the caret instead of showing a hand cursor.

- Standard Markdown only: note-taking syntax inherited from Zorite —
  `[[wiki links]]`, `#tags`, `key:: value` property panels, `((block refs))`,
  `^block-ids` and `![[embeds]]` — now displays as ordinary text. Their links
  never opened anything in mdoc. Saved files and Copy Markdown are unchanged
  (ADR 0030).

- Empty page: a blank Markdown tab shows a centered **Open files…** button,
  supported formats, the multi-file hint and ⌘O, and still accepts typing.
  Files dragged from the OS open when dropped on the empty page or on the
  sidebar, at an insertion line between documents; dragging over the collapsed
  rail reveals the list (ADR 0028).

- Chrome-style document sidebar: expanded by default with a + row under the
  last document. When collapsed, hovering or keyboard focus slides the full
  list out over the editor; it slides back shortly after the pointer leaves.
  Compact entries switch documents directly, replacing the filename popup
  (ADR 0027).

- First-use model setup asks in place: a Markdown-pane card for OCR and a
  Replacements-panel card for Pseudonymize show the download size, total-byte
  progress, Cancel and Retry, then continue the task. Settings no longer opens
  by itself; model choices save immediately with plain-language rows.
  English OCR (PP-OCRv6 Small) is the new default (ADR 0026).

- Multi-file opening with lazy retained tabs, paths-only session restoration,
  compact sidebar navigation, and automatic PDF/DOCX conversion when OCR is
  unnecessary; explicit inline OCR choices preserve the original source.
- Slim Markdown and PDF/DOCX overflow scrollbars, horizontal preview scrolling,
  and quieter expandable conversion notices.
- Product direction, roadmap, domain glossary, and consolidated architecture
  decisions; manual build-cache cleanup instructions.
- Read-only side-by-side DOCX preview with a local Rust converter, plus a
  read-only DOCX comment list that never enters editor text.
- Literal Markdown find (Cmd/Ctrl+F, Cmd/Ctrl+G navigation, Match case
  toggle) over rendered visible text.
- Local offline OCR import for scanned PDFs on Apple Silicon macOS and
  Windows x64, with explicit setup and review warnings.
- Search-bar styling pass and Markdown search performance coverage.

## 0.1.0

- Initial standalone mdoc repository, derived from Zorite.
- File-based Markdown WYSIWYG editing with atomic saves and external-change detection.
- Side-by-side PDF viewing and local document import through AnyDoc.
- Light and dark themes, with native file actions on macOS, Windows, and Linux.

For the original project's release history, see
[Zorite releases](https://github.com/packetThrower/zorite/releases).
