- **Tables are edited cell by cell.** In edit mode a tap on a table puts
  the caret in the cell tapped; typing, ⌫ and paste change only that cell
  (a typed pipe stays text, a line break becomes `<br>`), and nothing in
  the table is padded or realigned. Return goes to the cell below, Tab and
  ⇧Tab to the next and previous cell, and the arrow keys step from cell to
  cell. In a cell Aa shows table controls: add a row above or below,
  duplicate or delete a row, add or delete a column, align the column,
  move between cells and delete the table, each one step for Undo
  ([ADR 0109](docs/adr/0109-mobile-markdown-editor.md)). A `<br>` in a
  table cell now shows as a line break, as on desktop.
