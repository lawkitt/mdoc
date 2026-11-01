- **Invisible characters split no names.** A scan and the readiness check
  read past soft hyphens and zero-width characters, so a name with a soft
  hyphen inside is proposed whole, and read a Russian letter written with
  a combining mark as one letter ("е" and a diaeresis is "ё"); the document
  itself is unchanged (fixes #420; #418 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
