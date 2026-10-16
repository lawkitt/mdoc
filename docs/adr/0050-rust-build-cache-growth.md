# Rust build-cache growth

Status: Accepted, 2026-10-16.

## Problem

On 2026-10-16 the main `target/` held 150 GiB (135 GiB in `target/debug/deps`)
on a 512 GB Mac with 23 GiB free, and each temporary worktree had its own
`target/` that nothing removed. A week of agent transcripts showed no debugger
use; agents rely on test output and `RUST_BACKTRACE`.

The growth came mainly from artifact-hash churn: a toolchain bump, dependency
update, version bump or flag change builds a complete new copy under new hashes,
and Cargo never deletes the old one. Per-edit growth was already near zero:
under macOS's default `split-debuginfo = "unpacked"`, unchanged codegen units
are hardlinks to the same incremental objects.

## Decision

- **Lean dev debug info** in `[profile.*]` of the root `Cargo.toml`: line tables
  for workspace crates (`debug = "line-tables-only"`), none for dependencies
  (`debug = 0`), incremental compilation kept. Backtraces keep file and line;
  debugger variables are lost. Every build path (`cargo`, `just`,
  `scripts/perf.sh`, CI, rust-analyzer) uses it. Release settings are unchanged.
- **`split-debuginfo` stays at the platform default**: `packed` added ~2.6 s to a
  warm test build (3.8 → 6.4 s), over the agreed 10 % / 2 s limit.
- **Full debug info on request**: `[profile.debugging]` inherits dev with
  `debug = "full"` and `split-debuginfo = "packed"`, built into
  `target/debugging/` (`cargo run --profile debugging`), leaving the dev cache
  alone. Not `CARGO_PROFILE_DEV_DEBUG`, which rebuilds the dev cache both ways.
- **Recipes**: `just retire-worktree <path>` removes a worktree's `target/` and
  then the worktree; it refuses uncommitted, untracked or other ignored files
  and never forces. `just disk` reports each worktree's `target/` size. Resetting
  the main cache is a documented manual `cargo clean --profile dev`, recommended
  after a toolchain bump or large dependency update.
- **No shared `CARGO_TARGET_DIR`** (lock contention, branches invalidating each
  other), no cache caps, no automatic eviction. **sccache** only if `target/`
  exceeds about 30 GiB, measured with its own disk use.
- Guidance lives in `docs/development.md`; the project keeps no `AGENTS.md` or
  `CLAUDE.md`.

## Consequences

Each complete build configuration is about 45 % smaller (cold build 5.0 →
2.7 GiB, cold test 6.4 → 3.6 GiB) at no time cost. Stale copies still leave only
through manual cleaning. Linux and Windows keep their own `split-debuginfo`
defaults; CI caches shrink too.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0050-rust-build-cache-growth.md),
[measurements](../evidence/build-cache/2026-10-16/README.md).
