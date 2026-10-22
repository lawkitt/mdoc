# Enter starts a paragraph

Status: Accepted, 2026-10-22.

## Problem

The editor draws every `\n` as its own line, but CommonMark reads a single
`\n` between text lines as a soft break and joins them. Enter inserted one
`\n`, so two lines the editor showed apart reached the AI as one paragraph
through Copy for AI. Found in the 0.1.0 rc.1 run
([ADR 0057](0057-native-platform-acceptance.md)).

## Decision

The editor writes the Markdown that renders as what it shows; saved bytes and
Copy for AI stay the text as typed.

- **Enter makes a new block.** In prose and headings it inserts `\n\n`. A
  blank line, or the caret at a line's start, gets one `\n`, which splits
  soft-joined lines into paragraphs or adds a row. At the end of a line
  soft-joined to the next one, Enter also puts a blank line before that line.
  Repeated Enter adds empty rows; CommonMark collapses them, and nothing is
  joined or lost.
- **⇧Enter is a line break**: a CommonMark hard break, `\` then `\n`, which
  the editor hides away from the caret like other markers. In a list item the
  next line is indented to the item's body; in a quote it keeps the `>`
  prefix. In a heading, or with no text before the caret on the line, ⇧Enter
  is Enter.
- **Contexts keep their own Enter.**
  - *Lists:* Enter opens the next item with one `\n` (a tight list); Enter on
    an empty item exits, leaving a blank line.
  - *Quotes:* Enter starts a new quoted paragraph (`>` line, then the
    prefix); Enter on an empty quote line exits, removing the bare `>` line
    above and leaving a blank line.
  - *Code blocks and display math:* Enter and ⇧Enter insert one literal `\n`.
  - *Tables:* Enter moves to the cell below; from the last row it leaves the
    table with a blank line, since GFM reads a line right below a table as a
    row. ⇧Enter inserts `<br>` ([ADR 0046](0046-workspace-polish-from-anoni-review.md)).
- **⌫ and ⌦ undo a paragraph break in one press.** ⌫ at the start of a line
  below exactly one blank line, or ⌦ at the end of a line above one, removes
  the break and joins the two lines. It applies outside code, math and tables;
  extra blank rows are removed one at a time first.
- **Existing text is left alone.** Opening a document never rewrites its
  newlines; converted Markdown already separates paragraphs with blank lines.

## Invariants

- Typing Enter and ⇧Enter in prose, lists, quotes and tables renders in GFM
  as the editor shows it (`input::tests`, checked with the `markdown` crate).
- A trailing `\` hides only with text before it on its line and when not
  escaped. On a block's last line CommonMark shows it literally; the editor
  hides it there too.
