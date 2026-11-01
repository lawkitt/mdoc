- **Detection rules move into a rule pack.** The titles, roles, legal forms
  and date, phone and name shapes that decide what a scan keeps and what a
  manual selection is guessed to be now live in
  `crates/mdoc-detect/rules/{shared,en,ru}.toml`, checked by a unit test;
  detection behaves as before ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
