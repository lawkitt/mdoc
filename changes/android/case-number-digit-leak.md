- **A case number's digits stay out of amounts.** After "Номер дела: 7316",
  "7316 рублей" elsewhere is no longer proposed as the case number: a case
  number's repeats count only standing alone, never inside a longer number
  or identifier or as an amount
  ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
- **Only court cases are case numbers.** An archive, personnel or registry
  file ("архивное дело 58239", "личное дело № 4521") is no longer proposed
  as a case number (fixes #451).
