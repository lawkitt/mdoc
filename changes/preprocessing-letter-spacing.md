- **Letter-spaced names and footnoted labels are read.** A scan reads a
  letter-spaced name or label, as OCR and spaced-out signature captions
  write them ("D E L V A   R O S K", "И Н Н:"), as its words, and reads a
  label past its footnote reference or note marker ("**ИНН**[^1]:",
  "ИНН¹:"), so the value after it keeps its category; initials,
  one-letter words and enumerations read as before, and the document
  itself is unchanged (fixes #400 and #448;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
