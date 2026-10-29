# CI speed and script hygiene

Status: Accepted, 2026-10-26; amended 2026-10-28. Amends
[ADR 0037](0037-ci-and-release-pipeline.md) (CI jobs and cache). Runners,
target, test runner and the PR gate amended by [ADR 0082](0082-local-ci.md).

## Problem

A cold PR run took about 19 minutes on Windows: one job ran clippy (4.5 min)
and then the tests (14.5 min, of which the test build took 6 min and
`tests/headless_renders.rs` 7 min). Every PR ref saved its own ~1 GB cargo
cache, so a stack of PRs filled the 10 GB repository limit and evicted
`main`'s cache, and the next runs found none. Nothing linted or formatted the
24 shell scripts under `scripts/`; `actionlint` checks only workflow `run:`
blocks.

After the split, `test (windows-latest)` still took about 11m45s on most
runners: `tests/headless_renders.rs` alone took ~430 s, ~230 s on faster
hosted hardware. The render run spent its time in BGRA readback conversion
and PNG encoding in unoptimized dependencies, not in WARP's D3D work or app
layout. Runs for merged PRs and superseded `main` pushes kept holding runners.

## Decision

- **Target.** `test (windows-latest)` finishes in about 8 minutes or less on
  a warm cache, on the slower hosted runners too. The hosted Windows runners
  have 4 vCPUs, and below that WARP's software rasterization sets the floor.
  Windows stays on every PR, the render test included, with all of its
  screens, Tab-walk PNGs, frame costs and assertions.
- **Runners.** GitHub's free hosted runners only; no larger or self-hosted
  runners (a public repository: security and upkeep). Both test jobs log the
  runner's CPU model and logical core count; timings are compared only on
  the same CPU model.
- **Cache policy.** Only `main` saves cargo caches (rust-cache `save-if`);
  every other ref, PRs and tags included, restores `main`'s.
- **Concurrency.** CI and Actionlint replace a superseded run of the same PR
  or ref, for pushes to `main` and PR updates alike. A PR's `closed` event
  (merge or close) starts a run in the PR's group in which every job skips,
  cancelling the old run. CI called from `release.yml` passes
  `release-call: true` and runs in its own per-run group that is never
  cancelled: a called workflow inherits the caller's event and ref, so the
  event name cannot tell a release call apart.
- **Parallel jobs.** Clippy and the tests run as separate parallel jobs per
  shipped OS: shorter wall time for more runner minutes. Job names:
  `lint (ubuntu)`, `clippy (macos-26)`, `clippy (windows-latest)`,
  `test (macos-26)`, `test (windows-latest)`. `release.yml` still calls
  `ci.yml`, and the OCR smoke test stays a step of the test jobs. The render
  test gets no job of its own: with 4 vCPUs it would buy about a minute for a
  second Windows build on every PR.
- **Dependency optimization in the dev profile, per crate only.**
  Dependencies (`"*"`) stay at `opt-level` 0 with `debug = 0`; workspace
  crates stay unoptimized for debugging. A dependency gets its own
  `opt-level` only when a profile shows it hot and the cold local test build
  (`cargo test --workspace --features headless-renders --no-run`, empty
  target directory) grows no more than ~30% on macOS and Windows. Level 2,
  not 1: level 1 barely helped, since optimized crates otherwise call
  unoptimized generic helpers. Current overrides: the PDF rasterization crates,
  `gpui-pre-windows`, `png` and `simd-adler32` (readback conversion, PNG
  filters, Adler-32), and `gpui-pre-apple`, `image` and `fdeflate` (Metal
  readback, buffer copies, PNG compression; ADR 0082). Blanket levels stay
  rejected: on 2026-10-26 opt-level 1 for all dependencies grew the cold Mac
  test build from 77 s to 179 s (+134%; opt-level 2: 251 s).
- **Render pipelining.** On macOS and Windows the render process encodes PNGs
  on one background thread with a one-image queue; the summary is written
  only after the thread joins without error. No encoder processes.
- **Test runner.** `cargo test`, not cargo-nextest, for now.
- **Scripts.** No rewrite and no Rust `xtask`. `just lint-scripts` runs
  `shellcheck` and `shfmt -d` over `scripts/`, from `just check` and the
  Ubuntu lint job only. shfmt's flags match the existing style (two-space
  indent) and live in `.editorconfig`; `SHFMT_VERSION` is pinned in
  `scripts/tools.env`, installed by `just setup` and checked by `just doctor`.
  development.md keeps a short conventions note.
- **PowerShell.** No PSScriptAnalyzer for `ci-installed-windows.ps1`; every
  release run exercises it.

## Consequences

- `tests/headless_renders.rs` stays the longest step of `test (windows-latest)`.
  Measured on 2026-10-27/28 (#197): the local render run fell from 281 s to
  55 s (Ryzen 9 3900X); the cold Mac test build went from 79.7 s to 78.4 s and
  the cold Windows one from 146 s to 137 s. On CI, with only partial caches,
  the Windows job took 7m02s–8m43s on EPYC 9V45/9V74; the slower EPYC 7763
  and Xeon 8370C runners with a warm cache are not yet measured.
- A warm PR costs more runner minutes (two builds per OS), but Windows no
  longer runs clippy and the tests back to back.
- A PR that changes `Cargo.lock`, the toolchain or a dev-profile override
  builds partly cold until `main` saves a cache for the new key.
- A cancelled `main` run saves no cache; the newer run that replaced it does.
- Release runs on tags no longer save caches, so a rerun of a release's
  packaging builds cold (the `release` cache key never matches CI's). A tag's
  cache served only reruns of that same tag.
- There are no required status checks (no branch protection), so the renamed
  jobs and skipped `closed` runs break nothing.

References: [interview](https://github.com/lawkitt/mdoc/blob/cde5db2fab1ca383d388e18fada0ec0775e3035d/docs/design/scripts-and-ci.md),
[2026-10-27 interview](../design/ci-speed-2.md).
