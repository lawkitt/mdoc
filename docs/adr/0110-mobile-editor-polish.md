# Mobile editor polish

Status: Accepted, 2026-11-10. Amends
[ADR 0109](0109-mobile-markdown-editor.md) (edit mode, toolbar, tables) and
the ordered-list labels of [ADR 0096](0096-rendered-markdown-on-mobile.md).
Record:
[mobile editor polish grill](../design/mobile-editor-polish-grill-2026-11-10.md).

## Problem

The first phone use of edit mode (TestFlight `ios-v0.2.0-rc.4`) found it
unpolished: the pencil and Done threw the reader back to the top of the
text, Identities opened over the keyboard, a list button reached one
paragraph of several, table controls took a panel of fifteen keys, and
lists were faint and uneven.

## Decision

**Edit mode**

- **One text view** serves review, edit mode and the return from the
  Original: the scroll position, the caret and a selection survive the
  pencil, Done and the Text/Original switch.
- **Entering:** with a selection the keyboard comes up and the selection
  stays. With none, nothing moves: the bar changes and the keyboard waits
  for a tap in the text.
- **The bar while editing:** ☰, the title or the Text/Original switch, a
  tinted checkmark in its own capsule (Done), and ⋯. The Original keeps its
  own page and zoom between switches; opening it at the caret's page is a
  Later item.
- **Identities are hidden while editing.** iPhone: no button and no edge
  swipe until Done. iPad: the column gives way to the Original.

**Editing rules, in the editor core (desktop follows)**

- **A block command over several blocks** (list, task, heading, quote)
  applies to each selected block; blank lines in the selection are skipped
  and, for lists and tasks, the blank lines between the new items go. When
  every selected block is that kind already, the command turns them all
  back to text. A code block wraps the selection in one fence.
- **Italic on a bold selection** gives bold italic (`***x***`), and back.
- **Table caret:** a new row takes the caret in the same column; a new
  column takes it in the same row, and the grid scrolls to it.
- **Return in a table's last row** adds a row and takes the caret there in
  the same column. On a last row that is wholly empty it removes that row
  and leaves the table, as an empty item ends a list.
- **Ordered lists show the number written** (`9.` shows "9."; a nested
  `3.` shows "c." in its depth's style), on desktop too: a clause number
  is content, and Copy for AI sends it.

**Table controls**

- They live in the toolbar above the keyboard. With the caret in a cell the
  list and task buttons give way to row below, column right and a ⋯ menu:
  Row above, Column left, Delete row, Delete column, Align column (left,
  centre, right), Delete table. Nothing is drawn in the document.
- Undo and Redo sit in a capsule of their own at the toolbar's start and
  never scroll away; the buttons after them scroll (2026-11-11, after
  `rc.5`: in a cell Undo was cut off, and outside one Undo left the screen
  once the capsule was scrolled).
- A double tap or a long press on a cell selects the word there and shows
  the edit menu, and a selection handle dragged over the grid moves its
  end to the character under the finger (2026-11-11, after `rc.6`). The
  grid takes its own touches, to scroll sideways, so the text view's
  selection gestures never reached a cell; and UIKit moves a handle by
  TextKit's hit test, which finds the table's hidden source. The text
  view answers with the cell's character instead.
- With the caret in a cell, the text scrolls only to keep that cell in
  view (2026-11-11): UIKit scrolled to the hidden source, at the end of
  the grid, so a tap in a long table threw the cell out of view.
- The table panel goes: Aa is the formatting keyboard in a table too, so
  strikethrough, code and link work in cells. Duplicate row and the move
  keys leave the phone; the core keeps its commands.
- The formatting keyboard has no key back to the system keyboard: Aa
  switches back.

**Lists**

- Bullets, numbers and task boxes start flush with the paragraph's left
  edge; their text shares one column 24 pt in, and each nested level adds
  24 pt. Bullets and numbers take the text colour, the bullet at body size.
- The caret's line draws its marker in the same column, so its text does
  not move. A task keeps its box there: `- [ ]` fits the column only at an
  unreadable size, and the task button checks it (2026-11-11). A heading
  still shifts when its `#` shows (Later).

**Names**

- A command edits only what it changes: a list, task, heading or quote
  its line prefixes, and bold, italic and the other wraps their markers
  (2026-11-11, after `rc.5` and `rc.6`). An edit that runs into a
  proposed name drops it, so a command that replaced the whole selection
  dropped every name and alias in it.

## Invariants

- ADR 0109's hold: the text view's string is the source, saved bytes are
  the text as typed, and one rule exists once, in the core.
- No table command pads or realigns columns.

## Consequences

- Defects fixed with it, no decision involved: the caret's height on an
  empty line; text and the caret's line under the floating toolbar; an
  empty table column too narrow to tap; a long title cut at its head.
- The Android editor (ROADMAP Android item 10) starts from these rules.
- Shipped in five slices (#777, #778, #779, #783, #784), then one TestFlight
  build, `ios-v0.2.0-rc.5` (873), on 2026-11-11.

References: [list look variants](../evidence/mobile-editor/2026-11-10/list-look-variants.png).
