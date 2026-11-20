- **Hidden text and form field names stay out of imports, counted.** Text
  formatted as hidden in Word (DOCX, RTF and DOC), invisible text in a PDF
  and the internal names of PDF form fields never reach the document; the
  import note says how many, for example "2 hidden passages not imported."
  or "3 form field names not imported.". ODT comments are left out and
  counted too. Imports also pick up the converter fixes from upstream and
  the community: Word non-breaking hyphens, RTF deleted revisions, DOC
  checkboxes, duplicated PDF overlay text and Russian `а)` clause labels
  ([#925](https://github.com/lawkitt/mdoc/issues/925)).
