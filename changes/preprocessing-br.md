- **Values over HTML line breaks are found.** A scan reads a `<br>` inside a
  line as the line break it shows, and a value over the lines of a table
  cell ("27 Wisp Lane<br>Norwich NR2 8ZZ") is proposed as one span; a tag
  at a value's edge or any other tag stays syntax (fixes #402 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
