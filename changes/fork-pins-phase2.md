- **Repeated headers and footers appear once.** A PDF's running header or
  footer stays on the first page that shows it, and its exact repeats on
  later pages are left out; the import note says how many, for example "3
  repeated header and footer lines removed.". Rows that differ only in a
  number, such as two case numbers, are kept. PDF text drawn in its
  background's colour or covered by a box is left out too and counted with
  the other hidden passages. Imports also pick up the converters' table
  fixes (headings above a table, wrapped and page-split rows, a continued
  table's header row), paragraphs and turned pages on scans, small print on
  scans, Word list numbering and nested tables, and sparse Excel sheets;
  comment annotations no longer reach OCR's text
  ([#925](https://github.com/lawkitt/mdoc/issues/925)).
