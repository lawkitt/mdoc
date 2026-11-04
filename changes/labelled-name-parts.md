- **Names after name-part labels are proposed.** A value right after a
  label such as Surname, Given name, First name, Last name, Middle name,
  Patronymic, Фамилия, Имя or Отчество and a colon is proposed as a person
  when it looks like a name, also when two such pairs stand side by side
  on one line, as in an OCR'd two-column form ("Given name: Alvora
  Given name: Nerian"). A bare "Name:", a label in prose, placeholders,
  values with digits, code and link targets are left alone (#397;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
