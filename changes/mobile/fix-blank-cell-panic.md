- **A table with a blank cell no longer stops recognition.** In a document
  that names a person whose initials appear on their own, a Markdown table
  with a cell of only spaces (or a no-break space) made recognition fail
  with a panic; the blank cell is now read as
  empty ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
