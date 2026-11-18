- **Repeated headers and footers are counted, and case numbers stay.** A
  PDF's running header or footer stays on the first page that shows it, and
  the import note counts its repeats left out on later pages, for example
  "3 repeated header and footer lines removed."; scans of three or more
  pages now drop them too. Rows that differ only in a number, such as two
  case numbers, are no longer dropped as repeats, and a short scan keeps its
  small top or bottom line. PDF text drawn in its background's colour or
  covered by a box is left out and counted with the other hidden passages.
  Imports also pick up the converters' table fixes (headings above a table,
  wrapped and page-split rows, a continued table's header row), paragraphs
  and turned pages on scans, small print on scans, Word list numbering and
  nested tables, and sparse Excel sheets; comment annotations no longer
  reach OCR's text ([#925](https://github.com/lawkitt/mdoc/issues/925)).
