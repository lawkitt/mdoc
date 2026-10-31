- **A long document scans like its parts.** Detection windows hold whole
  paragraphs and each heading starts one, so a section repeated in a longer
  document gets the proposals it gets alone, and a repeated section is
  scanned once. Titles, role words and the names of categories are no longer
  proposed ("Ms", "her husband", "the Employer", "email"), nor a claim number
  as a bank account. A wording found only inside a longer one no longer takes
  an alias, so aliases don't skip a number (ORG_1–4, ORG_6)
  ([#302](https://github.com/lawkitt/mdoc/issues/302),
  [ADR 0002](docs/adr/0002-reviewable-local-pseudonymization.md)).
