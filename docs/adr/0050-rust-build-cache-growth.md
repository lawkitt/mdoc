# Rust build-cache growth

Status: accepted and implemented, 2026-10-16. The user agreed to the decisions
below in review; the object-file packaging was then chosen by the agreed
measurement rule. [Measurements](../evidence/build-cache/2026-10-16/README.md)
record the results.

## Problem

On 2026-10-16 the main `target/` held 150 GiB on a 512 GB Mac with 23 GiB free.
Almost all of it was `target/debug/deps` (135 GiB). Release output was 8.5 GiB,
incremental state under 1 GiB, and `~/.cargo` plus `~/.rustup` about 6.6 GiB.

The cause is specific to macOS. The dev profile defaults to
`split-debuginfo = "unpacked"`: binaries keep their DWARF in the per-codegen-unit
object files (`*.rcgu.o`) beside them, and every incremental rebuild writes a new
set under new names without deleting the old set. `deps` held about 569,000 such
files (≈202 GiB apparent). One `mdoc` test-binary hash alone had 17,920 object
files, about 70 rebuilds of 256 codegen units. Distinct artifact hashes add more:
104 `mdoc` and 37 `docx_worker` test binaries. Each temporary worktree also has
its own `target/` (9.7 and 11 GiB at the time) that nothing removes.

A search of the week's agent transcripts (Claude Code and Codex, 55 sessions,
2026-10-09 to 2026-10-16) found no debugger use (`lldb`, `gdb`, CodeLLDB).
Agents rely on test output and `RUST_BACKTRACE` (24 sessions), which line
tables preserve.

## Decision

- **Goal.** Growth per edit-and-rebuild cycle drops by an order of magnitude
  (the measured acceptance criterion), and `target/` stays under about 30 GiB
  after a week of normal development without manual cleaning. No cache caps and
  no automatic eviction.
- **Debug information.** The dev profile keeps line tables for workspace crates
  (`debug = "line-tables-only"`) and drops debug information for dependencies
  (`debug = 0`), with incremental compilation retained. Backtraces keep file and
  line; debugger variable inspection is lost. A committed opt-in profile restores
  full debug information when someone needs a debugger.
- **One place.** The settings live in `[profile.*]` of the root `Cargo.toml`, so
  `cargo`, `just`, `scripts/perf.sh` (`--profile test` inherits dev), CI and
  rust-analyzer build identically. Release settings are unchanged.
- **Retired worktrees.** A `just` recipe and documented step remove a worktree
  together with its `target/`. Rejected: a shared `CARGO_TARGET_DIR`, which saves
  disk but brings build-lock contention and branches invalidating each other's
  artifacts.
- **sccache.** Deferred to its own measured evaluation after this lands. It does
  not cache incremental workspace crates or linking, and adds its own disk use.
- **Reclaim.** The figures above are the "before" snapshot. The main `target/`
  is cleaned and its first rebuild serves as the cold-build baseline. Other
  worktrees' `target/` directories are removed only when those worktrees are
  retired.

- **Object-file packaging.** Measure line tables with `split-debuginfo =
  "unpacked"` and `"packed"`. Choose `packed` (one `.dSYM` per link, no
  accumulating `.o` files) if a warm edit → `cargo test -p mdoc --no-run` cycle
  slows by no more than 10 % or 2 s, whichever is larger; otherwise keep
  `unpacked` and rely on the cleanup recipes. Result: `packed` added ~2.6 s
  (3.8 → 6.4 s), so `split-debuginfo` stays at the platform default.
- **Measurement.** Three configurations (current defaults, line tables +
  `unpacked`, line tables + `packed`), each from a clean `target/` on an idle
  machine: cold `cargo build`; cold `just test`; ten warm cycles of a leaf edit
  in `src/` → `cargo test -p mdoc --no-run`; five warm cycles of an edit in
  `crates/mdoc-editor` → `just lint`. Record wall time and `target/` growth per
  step in `docs/evidence/build-cache/`.
- **Full debug information on request.** `[profile.debugging]` inherits dev
  with `debug = "full"` and `split-debuginfo = "packed"`, used as
  `cargo run --profile debugging`. It builds into `target/debugging/` and leaves
  the dev cache untouched. Rejected: overriding the dev profile through
  `CARGO_PROFILE_DEV_DEBUG`, which rebuilds the dev cache in both directions.
- **Recipes.** `just retire-worktree <path>` removes the worktree's `target/`
  and then the worktree; it refuses a worktree with uncommitted changes and
  never forces. `just disk` reports each worktree's `target/` size. A manual
  reset of the main cache is documented as `cargo clean --profile dev` rather
  than wrapped in a recipe.
- **Documentation.** Guidance lives in `docs/development.md` only; the project
  keeps no `AGENTS.md` or `CLAUDE.md`.

## Results

- Line tables with no dependency debug information make each complete build
  configuration about 45 % smaller (cold build 5.0 → 2.7 GiB, cold test 6.4 →
  3.6 GiB) with no time cost. Backtraces keep file and line.
- The per-cycle acceptance criterion turned out to measure the wrong thing.
  Under `unpacked`, each warm cycle still leaves ~750–1,000 new object-file
  names, but unchanged codegen units are hardlinks to the same incremental
  objects, so a small edit costs almost no bytes in any configuration. Byte
  growth per edit was already near zero; it did not drop tenfold.
- The 150 GiB came mainly from artifact-hash churn: a toolchain bump,
  dependency update, version bump or flag change builds a complete new copy
  under new hashes and Cargo never deletes the old one. The lean profile makes
  each copy ~45 % smaller; stale copies leave only through
  `cargo clean --profile dev`, which `docs/development.md` recommends after a
  toolchain bump or large dependency update. This stays manual.
- Whether `target/` stays under 30 GiB through a week of development is not
  yet observed; `just disk` makes it checkable.

## Consequences

- Debuggers show no variables in dev builds; `--profile debugging` restores
  them at the cost of a separate cold build into `target/debugging/`.
- The profile applies on every platform, so CI caches shrink too. Linux and
  Windows keep their own `split-debuginfo` defaults.
