# Changelog

All notable changes to **mdoc** are documented here.

## Unreleased

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
