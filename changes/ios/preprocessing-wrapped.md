- **Values wrapped over lines are found.** A name split by a line-end
  hyphen ("Brin-" then "sett") reads as one word for the scan and the
  readiness check, an email wrapped after its "@" too, and a name, email,
  phone or account number wrapped over lines is proposed as one span; two
  lines' text, a paragraph break or a hard line break is never joined
  (fixes #396 and #419; #388 in part;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
