- **Hidden text and form field names stay out of imports, counted.** Text
  formatted as hidden in Word (DOCX, RTF and DOC), invisible text in a PDF
  and the internal names of PDF form fields never reach the Markdown; the
  import note says how many, for example "2 hidden passages not imported."
  or "3 form field names not imported.". A form field keeps its value, and
  a ticked box reads ☑. ODT comments, which used to run into the text, are
  left out too and counted with Word's ("2 comments not imported.").
  Converter fixes from upstream and the community: a Word non-breaking
  hyphen is kept (`text-end`, not `textend`), RTF deleted revisions are
  left out, DOC checkboxes read ☐/☑, a stray `&` no longer drops a whole
  part, a broken `<` fails the import instead of silently cutting the text
  short, and oversized or deeply nested documents stop at the converter's
  resource limits instead of crashing. In PDFs, text in older form objects
  and some fonts with sparse character maps is read, duplicated overlaid
  text appears once, and Russian `а)`/`б)` clause labels become list items
  ([#925](https://github.com/lawkitt/mdoc/issues/925)).
