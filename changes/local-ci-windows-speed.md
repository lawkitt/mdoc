- **Faster local CI on Windows.** `local/windows` keeps the CI checkout's
  build cache between jobs (each job had built cold), runs at normal
  priority, builds GPUI's layout and paint crates at `opt-level` 2, and
  `just test` runs the script suites beside the Rust tests (fixes #240,
  [ADR 0082](docs/adr/0082-local-ci.md)).
