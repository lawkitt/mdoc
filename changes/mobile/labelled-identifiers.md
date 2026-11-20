- **Russian identifiers take their label's category.** A value after
  "ИНН" is a tax identifier and one after "СНИЛС" an identity number, even
  when its checksum fails or OCR misread a digit, and a table column headed
  by such a label counts as the label for its cells
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
