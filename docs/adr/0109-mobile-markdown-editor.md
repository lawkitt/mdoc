# Mobile Markdown editor

Status: Accepted, 2026-11-09. Amended by
[ADR 0110](0110-mobile-editor-polish.md) (entering edit mode, Identities,
table controls, lists). Amends
[ADR 0085](0085-mobile-apps.md) (scope: "no full Markdown editor"; review
screen "No text editing"), [ADR 0096](0096-rendered-markdown-on-mobile.md)
("The text stays read-only") and [ADR 0098](0098-mobile-review-bar.md)
("Never: … any other editing"). Record:
[mobile editor grill](../design/mobile-editor-grill-2026-11-09.md).

## Problem

The phone can review a document but not change it: no fixing OCR damage,
no cutting a page of boilerplate, no writing a new document. The desktop
editor (`mdoc-editor`) is GPUI, which has no iOS or Android backend. The
target feel is Obsidian's iOS live-preview editor.

## Decision

- **Scope: edit any document, as desktop does.** Imported, scanned, pasted
  and new blank documents are editable. Review keeps working over the
  edits: proposals and decisions follow them, as `mdoc-pii` already does
  for desktop. Not a notes app: no tags, wikilinks or vault.
- **iOS first.** Android uses the same core in this milestone; its editor
  screen is its own Android roadmap item, after its rendered Markdown.
- **Native TextKit 2 plus a shared Rust editor core.** The rendered text
  view becomes editable. Editing rules (Enter and list continuation, joins
  on ⌫/⌦, formatting toggles, indent and outdent, turn into, task toggle,
  table commands, paste normalisation, the coalescing undo history) move
  out of `mdoc-editor` into a GUI-free crate that desktop and both phones
  call, so each rule exists once (ADR 0104). Plain typing is applied
  natively and reaches the core as coalesced edits; structural keys call
  the core directly.
- **Reveal rule on the phone:** the span the caret touches shows its
  markers dimmed; line prefixes show on the caret's line. The core takes
  the rule as an option; desktop keeps "formatting markers never reveal"
  for now.
- **Edit mode.** An Edit button switches the review screen to edit mode
  (keyboard and toolbar; underlines dimmed and not tappable; review bar
  hidden); Done returns to review. A new blank document opens in edit
  mode. Edit mode works with Show Markdown on or off.
- **Saving:** autosave after typing pauses (~1 s), on leaving the document
  and on going to the background; no Save button. Undo and redo keep the
  core's 256 steps for the open session; no version history.
- **Names in new text:** discovery runs after each typing pause;
  recognition runs only on demand ("Find names again").
- **Toolbar:** a capsule bar above the keyboard (undo, redo, bold, italic,
  list, task, indent, outdent, Aa, hide keyboard); Aa swaps in a
  formatting keyboard (Turn into H1–H3, quote, code, callout; strike,
  code, highlight, link, rule, table). In a table the toolbar carries the
  table controls (ADR 0110). The selection menu stays the system one.
- **Tables** are edited cell by cell in the drawn grid: typing changes only
  that cell's bytes; row and column actions are the core's table commands;
  nothing pads or realigns columns.
- **Typing helpers:** Return continues and ends lists, tasks, numbers and
  quotes; code fences close themselves; no auto-pairing; smart quotes and
  dashes off; system autocorrect and dictation on.
- **Clipboard:** in edit mode Cut and Copy use a pasteboard only mdoc
  reads; Paste reads it first, then the system clipboard. Look Up,
  Translate, Search Web, Share-from-selection and Writing Tools stay off;
  plain text leaves only through Copy for AI and Share.
- **Edits over names** behave as on desktop: an edit overlapping an
  applied alias stops tracking that mention; an edit inside a pending
  candidate drops it, and discovery may propose it again. Edit mode tints
  applied aliases and explains the first broken one in a one-line hint.
- **Links:** in edit mode a tap places the caret; a chip offers Open and
  Edit; the Link button opens a text + URL sheet.
- **New document:** first in Home's + menu; opens in edit mode; named
  after its first line until renamed; an untouched empty one is discarded.
- **Paste, images, find, spelling:** rich paste through `pasted_markdown`
  (ADR 0105); existing images can be deleted, none inserted; the system
  find bar in both modes, Replace in edit mode as core edits; system
  spellcheck in edit mode only.
- **Hardware keys:** ⌘E, ⌘B, ⌘I, ⌘K, ⌘Z, ⇧⌘Z, Tab/⇧Tab (cells in a
  table), ⌘F. ⌘N opens a new document (amended 2026-11-10; it went to
  Start under ADR 0102, whose menu label already read "New document").
- **Acceptance:** no dropped frame per keystroke on the iPhone 12 Pro in
  the 37-page `legal-opinion.docx` fixture; restyle under 8 ms a
  keystroke, structural core calls under 16 ms; a typing fuzz test keeps
  saved bytes equal to the view's string; VoiceOver editing, Dynamic Type
  to accessibility sizes and an accessibility audit in edit mode; the
  extracted core passes desktop's editor tests.
- **One undo line:** typing joins the core's undo line as coalesced
  "Typing" steps beside review decisions, as on desktop; edit mode's undo
  and the review bar's Undo both undo the latest step, and the toast names
  it.
- **No editing during a job:** Edit is disabled while OCR or recognition
  runs for the document; Find names again starts only outside edit mode.
- **Replies stay read-only:** the Reply view and Restore with mdoc show
  output and are not editable.
- **Original beside the text:** the iPad keeps the Original side by side
  in edit mode; on iPhone the Text/Original switch stays, and Original
  hides the keyboard without leaving edit mode.
- **No new settings:** Show Markdown stays the one display setting; the
  sample document is editable and Try a sample restores it.
- **Invariants kept:** the text view's string is the source; saved bytes
  are the text as typed (ADR 0030, ADR 0060); candidate offsets are the
  core's UTF-8 source offsets; one Markdown parser (`mdoc-markdown`).

## Consequences

- Rejected: CodeMirror 6 in a web view (Obsidian's stack: a second
  parser, rules outside the core, web accessibility) and rules written in
  Swift (breaks ADR 0104, doubles Android).
- Ships as iOS 0.3 after the current freeze, in six slices: core
  extraction (desktop switched); edit mode with Show Markdown on;
  live-preview editing; toolbar, formatting keyboard and links; tables;
  new document, Find names again and iPad keys.
- Desktop keeps its reveal rule for now; a ROADMAP "To grill" item
  revisits it after phone use.
- Desktop changes too: `mdoc-editor` calls the extracted core, pinned by
  its existing tests.
