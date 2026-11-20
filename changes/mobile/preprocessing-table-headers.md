- **Identifier columns keep their header.** A scan reads each value in a
  Markdown table column headed as an identifier (Паспорт, ИНН, Счёт,
  Телефон, Tax ID, IBAN, Phone and the like) with its header ("Паспорт:
  46 82 731609"), so a passport number alone in a cell is no longer
  proposed as a phone; name and other columns read as before, and the
  document itself is unchanged (fixes #397 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
