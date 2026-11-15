- **The merge gate no longer drops a CI change whose group partner merged
  first.** When a queue commit's areas shrink from desktop and CI to CI
  alone (another PR in its group reached `main` first), the gate now
  counts `ci/scripts` as passed when that check never ran but `ci/macos`
  passed on the commit, since `ci/macos` runs the same script suites; the
  description says "scripts (via ci/macos)" (fixes #665,
  [ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).
