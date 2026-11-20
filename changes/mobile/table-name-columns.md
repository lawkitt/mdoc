- **Names in table columns are proposed.** A cell under a Markdown table
  header that names a part of a person's name (Фамилия, Имя, Отчество,
  ФИО, Surname, Given name, Middle name, Full name and the like, or Name
  beside one of them) is proposed as a person when it looks like a name,
  so a short surname alone in its cell ("Кий", "Лю") is no longer missed.
  Placeholders, totals rows, role words and a lone "Name" column of files
  or companies are left alone (#397;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
