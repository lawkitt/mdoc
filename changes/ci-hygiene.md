- **Hosted CI hygiene.** CI jobs time out after 15 (lint), 20 (clippy) and
  30 (test) minutes, `just test` runs the workspace tests with cargo-nextest
  (per-test timeouts, doctests through `cargo test --doc`), `main` deletes
  older generations of its cargo caches, renders artifacts are kept 7 days,
  and PRs on any base branch get CI
  ([ADR 0082](docs/adr/0082-local-ci.md); fixes #231).
