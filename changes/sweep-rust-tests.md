- **CI: the recognition fixtures check on every core.** Their one test, the
  only Rust test besides the render test over 10 s on hosted macOS (15 to
  17 s), now spreads the fixtures over the machine's cores: about 6 s with
  three and 5 s on the PC instead of 20. The plain unit tests that took 22
  to 31 s on the PC run in under 10 s alone; they were slowed by the script
  suites beside them, which ADR 0112 moves off the Windows job
  ([ADR 0112](docs/adr/0112-ci-speed-after-the-first-fortnight.md)).
