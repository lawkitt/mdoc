- **Scans lose their repeated headers and footers, as on the desktop.** A
  scan's running header or footer now stays on the first page only, and the
  import note counts the rest ("6 repeated header and footer lines
  removed."), though pages are still recognized one at a time and an
  import resumes after a stop. Header numbers that rise page by page, such
  as "Invoice 4171" to "4174", or follow "No.", "№" or a word like
  "Invoice", stay; and "a - b" in a PDF keeps its spaces
  ([#925](https://github.com/lawkitt/mdoc/issues/925),
  [#952](https://github.com/lawkitt/mdoc/issues/952)).
