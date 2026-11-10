# Mobile editor polish grill, 2026-11-10

Interview record for the polish of iOS edit mode ([ADR 0109](../adr/0109-mobile-markdown-editor.md)),
after the maintainer's first use of TestFlight build rc.4 on the iPhone 17
Pro. Decisions go into an amendment to ADR 0109 once the tree is closed;
this file keeps the findings, the answers and the open branches.

## Findings (iPhone 17 Pro Simulator, main f08bbb7d)

Reproduced with a throwaway XCUITest (not committed) seeded with a Russian
form of paragraphs, tables and lists.

**Reported by the maintainer, all reproduced**

1. *Tiny caret* on every empty line (after Return, after a heading). Cause
   not traced; the empty line probably takes the hidden markers' near-zero
   font.
2. *Top bar:* "Done" is plain text in one capsule with two icons.
3. *Italic removes bold* on a selection: `**два**` + Italic gives `*два*`.
   `format::unwrap_edit` reads one `*` of the `**` beside the selection as
   an italic marker. A caret alone in a bold word is right (`***три***`).
4. *Pencil and Done scroll to the top* and drop the selection:
   `DocumentScreen.textPane` holds the text column in two branches of an
   `if`, so a change of `isEditing` builds a new text view.
5. *List and checklist reach only the first paragraph:* `commands::turn_into`
   takes the block at the selection's start. A selection that starts on a
   blank line makes an empty `- `.
6. *List look:* small grey bullets flush left; text starts at 18, 21 and
   23 pt for bullets, numbers and tasks; the caret's line shifts sideways
   when its marker shows.
7. *Row above and Row below* put the caret in the new row's first cell
   (`tables::insert_row`, shared with desktop).
8. *Identities stay live in edit mode:* the button and the right-edge swipe
   open the drawer with the keyboard and the edit toolbar over it; Replace
   works.

**Found during the run**

- A long title is cut at its head ("ettlement agreem…") and runs under ☰
  and the trailing buttons, in both modes.
- Text and the caret's line pass under the floating edit toolbar; after a
  format command the caret's line can sit behind it.
- A column added to a table is about 25 px wide while empty.
- The table controls clip their Move row and leave half of each row empty.
- The formatting keyboard's key back to the system keyboard sits in its
  Insert section.
- A heading shifts sideways when its `#` shows.

Plain fixes, no decision needed: 1, 3, the text under the toolbar, the
narrow column, the title.

## Round 1 (answered 2026-11-10)

- **Q1 Top bar in edit mode.** Done becomes a tinted checkmark in its own
  capsule; Identities are hidden. *Open:* the maintainer switches between
  the text and the Original while fixing a poor scan's text, so the
  Text/Original switch cannot simply go (round 2).
- **Q2 Identities while editing, iPhone:** hidden. No button, and no edge
  swipe until Done. As ADR 0109 hides the review bar.
- **Q3 Identities while editing, iPad:** the column gives way to the
  Original.
- **Q4 Entering edit mode:** the text view is kept, so the position and a
  selection survive both ways. With no selection nothing moves: the bar
  changes and the keyboard waits for a tap in the text. With a selection
  the keyboard comes up and the selection stays.
- **Q5 List and checklist over several paragraphs:** each selected
  paragraph becomes an item of its own, the blank lines between them go,
  blank lines are skipped. When every selected block is an item already,
  the button turns them all back to text. Headings and quotes follow the
  same rule; a code block wraps the selection in one fence. In the shared
  core, so desktop follows.
- **Q6 Caret after table commands:** a new row takes the caret in the same
  column; a new column takes it in the same row. In the shared core.
- **Q7 List look:** one text column for bullets, numbers and tasks (about
  24 pt, nested levels 24 more); bullets and numbers in the text colour,
  the bullet at body size; the caret's line draws its marker in the same
  column, so the text stays put. Variants are rendered for the maintainer
  to pick from before it is built.
- **Q8 Table controls:** not settled. The maintainer wants something
  simpler than Bear's dots in the cell (a ⋮ in the caret's cell that opens
  one menu) (round 2).

## Round 2 (answered 2026-11-10, all recommendations accepted)

Facts checked first: switching to Original in edit mode throws the text
view away today (the same fault as finding 4); the app has no link between
a place in the text and a page of the Original; Return in a cell moves to
the cell below and, from the last row, leaves the table.

- **Q9 The Original while editing, iPhone:** the Text/Original switch
  stays in edit mode. Original hides the keyboard and keeps edit mode on;
  Text comes back to the same caret and position. The bar is ☰, the
  switch, ✓, ⋯. A split view leaves each half about four lines with the
  keyboard up; a peek key needs a page mapping that does not exist.
- **Q10 The Original at the caret's page:** not now. The Original stays
  where the reader left it. Opening a scan at the caret's page (from its
  page headings) is a later item.
- **Q11 Where table controls live:** in the toolbar above the keyboard.
  With the caret in a cell, the list and task buttons give way to three:
  row below, column right, and a ⋯ menu. Nothing is drawn in the document
  and the table panel goes. Aa is the formatting keyboard in a table too,
  so strikethrough, code and link work in cells.
- **Q12 The ⋯ menu:** Row above, Column left, Delete row, Delete column,
  Align column (left, centre, right), Delete table. Duplicate row and the
  four Move keys go from the phone; the core keeps its commands.
- **Q13 Return in the last row:** adds a row and takes the caret there in
  the same column; on a last row that is wholly empty it removes that row
  and leaves the table, as an empty item ends a list. In the shared core,
  so desktop follows.
- **Q14 A new column out of view:** the grid scrolls to the caret's new
  cell.

## Round 3 (answered 2026-11-10, all recommendations accepted)

Found while rendering the list variants: an ordered list shows positions,
not the numbers written (`9.`, `10.` show as "1.", "2."); the label is
counted in the shared core, so desktop does the same.

- **Q15 List look:** variant 1 of the three rendered
  ([evidence](../evidence/mobile-editor/2026-11-10/list-look-variants.png);
  left to right: today, 1, 2, 3): bullets and numbers in the text colour,
  flush with the paragraph's left edge, all text in one column 24 pt in.
  An 8 pt inset (variant 2) wraps a long task to three lines; grey markers
  (variant 3) stay faint.
- **Q16 Numbers in ordered lists:** the number written shows, on desktop
  too. A clause number is content, and Copy for AI sends the written
  number.
- **Q17 The formatting keyboard's Keyboard key:** removed; Aa switches
  back.
- **Q18 Heading shift when `#` shows:** left as it is, a Later item. `##`
  and `###` do not fit the 18 pt margin.
- **Q19 Slices:** five PRs: (1) core; (2) iOS modes; (3) iOS caret and
  toolbar; (4) iOS table controls, after 1; (5) iOS list look, after 1.
  Slice 1 on `wsl`, 2 and 3 on the Mac, then 4 and 5. One hub session with
  a Codex review before each PR.
- **Q20 TestFlight:** one build after all five merge and the iPhone 12 Pro
  gate passes.

The decisions bind the Android editor when it is built (ROADMAP Android
item 10). The tree is closed: [ADR 0110](../adr/0110-mobile-editor-polish.md),
ROADMAP iOS item 24.
