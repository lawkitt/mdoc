- **CI-only changes skip the desktop jobs.** A queue commit that touches only
  the CI machinery (the merge gate, the queue runner, the CI scripts and
  workflows, their suites) runs `ci/scripts`, the script suites on Linux and
  under `/bin/bash` 3.2, beside `lint (ubuntu)`, instead of `ci/macos`,
  `ci/windows` and the detection gate; a change to `ci-ios.yml` now runs
  `ci/ios` ([ADR 0100](docs/adr/0100-ci-reliability-after-the-cutover.md)).
