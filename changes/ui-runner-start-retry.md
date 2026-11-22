- **A hosted iOS UI part retries a test runner that failed to start.** When
  the runner crashes while preparing to run tests or times out initializing
  for UI testing, before any test, the part runs once more without
  building and passes with a warning (fixes #1065).
