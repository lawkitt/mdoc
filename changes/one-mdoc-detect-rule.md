- **`mdoc-detect`'s unit tests build the rule pack once.** The `unit` and
  `rust` test tiers run `mdoc-detect`'s lib tests in one process instead of
  one nextest process each, so the rule pack is no longer built for every
  test ([ADR 0113](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
  Quarantined tests are skipped by exact name, and failures are named as
  nextest names them.
