- **Editing polish.** "Turn into" applies to every block a selection
  touches, skipping blank lines and closing up new list items, and turns
  them all back into text when each is that kind already; a code block
  wraps the selection in one fence. Italic on a bold selection gives bold
  italic, and Bold on an italic one too. A new table row or column takes
  the caret into its own cell in line with where it was; Return in a
  table's last row adds a row, and on an empty last row removes it and
  leaves the table. Ordered lists show the number written (`9.` shows 9.,
  a nested `3.` shows c.), as Copy carries it
  ([ADR 0110](docs/adr/0110-mobile-editor-polish.md)).
