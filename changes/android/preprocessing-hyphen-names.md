- **A surname split by a line-end hyphen joins its name.** Where a known
  person's name recurs with a word split over lines ("Верес-" then "това
  Дарья Ильинична"), the whole name is proposed as one span in that
  person's identity, not just the first name and patronymic; a capital
  after the hyphen ("Санкт-" then "Петербург"), a dash or a plain line
  break keeps the parts apart (fixes #388;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
