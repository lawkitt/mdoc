- **Table cells keep their column header.** A scan reads each cell of a
  Markdown table with its column header ("Фамилия: Кий"), so a surname,
  a first name or a passport alone in a cell is found and named for what
  it is; the document itself is unchanged (fixes #397 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
