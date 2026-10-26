# CI speed and script hygiene

Status: Accepted, 2026-10-26. Amends [ADR 0037](0037-ci-and-release-pipeline.md)
(CI jobs and cache).

## Problem

A cold PR run took about 19 minutes on Windows: one job ran clippy (4.5 min)
and then the tests (14.5 min, of which the test build took 6 min and
`tests/headless_renders.rs` 7 min). Every PR ref saved its own ~1 GB cargo
cache, so a stack of PRs filled the 10 GB repository limit and evicted
`main`'s cache, and the next runs found none. Nothing linted or formatted the
24 shell scripts under `scripts/`; `actionlint` checks only workflow `run:`
blocks.

## Decision

- **Target.** A PR's CI finishes in about 10 minutes or less on a warm cache.
  Windows stays on every PR, the render test included.
- **Runners.** GitHub's free hosted runners only; no larger or self-hosted
  runners (a public repository: security and upkeep).
- **Cache policy.** Only `main` saves cargo caches (rust-cache `save-if`);
  every other ref, PRs and tags included, restores `main`'s.
- **Parallel jobs.** Clippy and the tests run as separate parallel jobs per
  shipped OS: shorter wall time for more runner minutes. Job names:
  `lint (ubuntu)`, `clippy (macos-26)`, `clippy (windows-latest)`,
  `test (macos-26)`, `test (windows-latest)`. `release.yml` still calls
  `ci.yml`, and the OCR smoke test stays a step of the test jobs.
- **Dependency optimization.** The dev profile compiles dependencies (`"*"`)
  at the `opt-level` (1 or 2) that most shortens the warm Windows test job,
  provided a cold local build grows no more than ~30%; dependencies keep
  `debug = 0`. Workspace crates stay unoptimized for debugging.
- **Scripts.** No rewrite and no Rust `xtask`. `just lint-scripts` runs
  `shellcheck` and `shfmt -d` over `scripts/`, from `just check` and the
  Ubuntu lint job only. shfmt's flags match the existing style (two-space
  indent) and live in `.editorconfig`; `SHFMT_VERSION` is pinned in
  `scripts/tools.env`, installed by `just setup` and checked by `just doctor`.
  development.md keeps a short conventions note.
- **PowerShell.** No PSScriptAnalyzer for `ci-installed-windows.ps1`; every
  release run exercises it.

Until the ROADMAP's *CI speed and script hygiene* item is done, the
dependency optimization and `just lint-scripts` are being adopted, not yet in
place.

## Consequences

- A warm PR costs more runner minutes (two builds per OS), but Windows no
  longer runs clippy and the tests back to back.
- A PR that changes `Cargo.lock` or the toolchain builds cold until `main`
  saves a cache for the new key.
- Release runs on tags no longer save caches, so a rerun of a release's
  packaging builds cold (the `release` cache key never matches CI's). A tag's
  cache served only reruns of that same tag.
- There are no required status checks (no branch protection), so the renamed
  jobs break nothing.

References: [interview](../design/scripts-and-ci.md).
