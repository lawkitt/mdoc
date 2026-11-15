- **CI-only changes skip the desktop jobs.** A queue commit that touches only
  the CI machinery (the merge gate, the queue runner, the CI scripts and
  workflows, their suites) runs `ci/scripts`, the script suites on Linux and
  under `/bin/bash` 3.2, beside `lint (ubuntu)`, instead of `ci/macos`,
  `ci/windows` and the detection gate; a change to `ci-ios.yml` now runs
  `ci/ios` ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).
