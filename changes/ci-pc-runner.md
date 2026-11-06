- **The PC's queue jobs explain their failures and keep a week of logs.** A
  failing Android job's status names the failing test from Gradle's JUnit
  results, a failed job's summary ends with the failing step's last lines,
  a passing Windows job drops its renders so 7 days of logs fit, the
  runner keeps the tailnet log page served and reports when it is not, and
  the Android CI job builds the core for the emulator's x86_64 only
  ([ADR 0100](docs/adr/0100-ci-reliability-after-the-cutover.md), #614).
