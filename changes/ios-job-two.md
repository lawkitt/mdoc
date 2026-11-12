- **CI: the iOS job runs in two parts.** `ci/ios (unit)` runs the unit and
  parity tests, then the Release build that `ci/ios-release` ran;
  `ci/ios (ui)` runs the UI tests beside it. The two typing-speed
  measurements run only in the nightly, and `just ios-test` takes
  `-only-testing` identifiers (ADR 0112).
