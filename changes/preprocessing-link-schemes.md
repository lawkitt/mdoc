- **Links into legal databases are not read as text.** A link target of a
  scheme other than web, email or phone (`consultantplus:`, `garant:`,
  `file:`) is a reference into another program: a scan no longer reads its
  hashes and short numbers, so a ConsultantPlus `ref=` hash is not proposed
  as a tax or bank number. A name, an email or a long number (a phone, a tax
  number) in such a target still reads (#494;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
