- **Hosted iOS UI tests retry once.** A failed test in `ci/ios (ui 1)` or
  `(ui 2)` runs once more (`-retry-tests-on-failure -test-iterations 2`);
  a pass on the retry keeps the job green but is named in a warning
  ("Passed on retry in ci/ios (ui 2): <test>") and the job summary.
  `scripts/ci-stats.sh` reads `ci/ios`'s annotations, so a hosted iOS
  failure gets ADR 0112's same-test-elsewhere rule, and lists retried
  passes and the tests with two unrelated failures to quarantine. Unit,
  Rust, Android and script tests still never retry
  ([ADR 0113](docs/adr/0113-ci-targets-by-commit-and-detection-off-macos.md)).
