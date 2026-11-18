- **Invoice and case numbers in a PDF's header stay, and so do spaced
  dashes.** A header number that rises page by page, such as "Invoice 4171"
  to "4174", or one after "No.", "№" or a word like "Invoice", is no longer
  dropped as a repeated header or a page number; the copy of a repeated
  header that stays is always on a page that is imported; and "a - b" keeps
  its spaces instead of becoming "a-b"
  ([#925](https://github.com/lawkitt/mdoc/issues/925),
  [#952](https://github.com/lawkitt/mdoc/issues/952)).
