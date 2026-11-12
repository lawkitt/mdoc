- **CI: the PC's Windows job runs only the Rust tests.** `ci/windows` now
  runs `just test-tier rust` (every Rust test tier in one nextest run, then
  the doctests) instead of `just test`; the script suites stay on hosted
  Linux and macOS. They took about 25 of the job's 30 to 45 minutes and
  failed most of its runs, so its limit goes back to 30 minutes
  ([ADR 0112](docs/adr/0112-ci-speed-after-the-first-fortnight.md)).
