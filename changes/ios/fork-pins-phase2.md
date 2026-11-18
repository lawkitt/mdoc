- **Repeated headers and footers appear once.** A PDF's running header or
  footer in its text layer stays on the first page that shows it, and its
  exact repeats are left out and counted in the import note, for example "3
  repeated header and footer lines removed."; a scan keeps them on every
  page. A scanned page's small top or bottom line is no longer dropped. PDF
  text drawn in its background's colour or covered by a box is left out and
  counted with the other hidden passages. Imports also pick up the
  converters' table fixes, paragraphs and turned pages on scans, small
  print on scans, Word list numbering and nested tables, and sparse Excel
  sheets ([#925](https://github.com/lawkitt/mdoc/issues/925)).
