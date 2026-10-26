# Scripts and CI interview (2026-10)

Working record of the interview after ADR 0069. Per ADR 0052 it leaves the
tree once its decisions live in an ADR and the ROADMAP; link it by permalink.

Baseline: `main` at a7ecca6.

## Findings (facts, not decisions)

- `scripts/`: 24 files, 1,491 lines. 20 source `scripts/lib.sh` (which sets
  the shell options); two-space indent; `shellcheck` reports only SC1091 (info,
  sourced file not followed). Nothing lints or formats the scripts themselves;
  `actionlint` shellchecks workflow `run:` blocks only. `shfmt` is not
  installed or pinned.
- CI (`ci.yml`): `lint (ubuntu)` runs fmt-check, deny, `just lint`; `test
  (macos-26)` and `test (windows-latest)` run `just lint` then `just test` in
  one job. Cold run on #159: Windows lint 4.5 min (clippy build 3m23s), test
  14.5 min (test build 6m15s, `tests/headless_renders.rs` 6m51s, all unit
  tests < 1 min); macOS 2 + 6.5 min; Ubuntu 2 min.
- Actions cache: at the 10 GB repository limit, almost all per-PR
  `refs/pull/N/merge` entries of ~0.9–1 GB. Main's cache had been evicted, so
  the stack's runs found no cache.
- Dev profile: deps get `debug = 0`; only the PDF rasterization crates get
  `opt-level = 2`.

## Decisions

- **D1 Scripts scope.** No rewrite. Light upkeep: `shellcheck` and `shfmt`
  over `scripts/` in the lint gate, and a short conventions note in
  development.md. A Rust `xtask` costs more than glue scripts justify.
- **D2 CI target.** A PR's CI finishes in about 10 minutes or less on a warm
  cache; Windows stays on every PR.
- **D3 Cache policy.** Only `main` saves caches (rust-cache `save-if`); PRs
  restore main's. A large stack can no longer evict main's cache.
- **D4 Parallel jobs.** Clippy and tests run as separate parallel jobs per OS:
  shorter wall time for more runner minutes.
- **D5 Windows render test.** Stays on every PR; make it faster by compiling
  dependencies with optimization in the dev/test profile, measured before and
  after.
- **D6 Runners.** GitHub's free hosted runners; no larger or self-hosted
  runners (public repo: security and upkeep).
- **D7 Where the script lint runs.** A `just lint-scripts` recipe
  (`shellcheck` + `shfmt -d` over `scripts/`), run by `just check` and the
  Ubuntu lint job only. `SHFMT_VERSION` is pinned in `scripts/tools.env`;
  `just setup` installs it and `just doctor` checks it.
- **D8 shfmt style.** Flags that match the current style (two-space indent)
  so the first pass is the smallest diff; kept in `.editorconfig`, which
  shfmt reads.
- **D9 PowerShell.** No PSScriptAnalyzer for `ci-installed-windows.ps1`; every
  release run exercises it.
- **D10 Job names.** `lint (ubuntu)`, `clippy (macos-26)`,
  `clippy (windows-latest)`, `test (macos-26)`, `test (windows-latest)`.
  `release.yml` still calls `ci.yml`; the OCR smoke test stays a step of the
  test jobs. No branch protection, so no required-check names break.
- **D11 Dependency opt-level rule.** Try `opt-level` 1 and 2 for `"*"` in the
  dev profile (keeping `debug = 0`); adopt the one that most shortens the warm
  Windows test job, if a cold local build grows no more than ~30%. If the
  render test stays slow because time is in workspace crates, stop and report
  the numbers; workspace crates stay unoptimized for debugging.
- **D12 Records and PRs.** ADR 0070 "CI speed and script hygiene" amends ADR
  0037's CI section; ROADMAP's Later `scripts/` item becomes an Engineering
  item linking it. Three PRs, each Codex-reviewed: (1) cache policy and split
  jobs in `ci.yml`, with the ADR; (2) dependency opt-level with before/after
  numbers; (3) `lint-scripts` and the shfmt pass. This record is deleted by
  the last PR.
