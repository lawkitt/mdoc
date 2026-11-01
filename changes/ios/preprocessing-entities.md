- **Character references hide no names.** A scan and the readiness check
  read HTML character references as their characters, so "Ann&#97;
  Petrova" is proposed as one name and "ismay&#64;crane-pith.invalid" as
  an email, and a leftover written with references still counts; the
  document itself is unchanged (fixes #367;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
