# Build-cache measurements, 2026-10-16

Measurements for [ADR 0050](../../../adr/0050-rust-build-cache-growth.md).
Apple M4 (10 cores, 24 GiB), macOS 15.6, Rust 1.99.0, commit `73f83f9`, idle
machine. Raw rows: [results.tsv](results.tsv). Harness: [measure.sh](measure.sh),
run in a throwaway worktree.

## Configurations

- **default**: the profile as committed before ADR 0050 (`debug = 2` everywhere,
  macOS `split-debuginfo = "unpacked"`).
- **unpacked**: `debug = "line-tables-only"` for workspace crates, `debug = 0`
  for dependencies, `split-debuginfo = "unpacked"`.
- **packed**: the same with `split-debuginfo = "packed"`.

Every cold step starts from an empty `target/`. Warm cycles change a real
function body: `src/recents.rs` (mdoc only), or `shape_cell_lines` in
mdoc-editor, which mdoc then rebuilds against. Times are means; the first warm
cycle of each loop is excluded because it builds that loop's first incremental
session (the first lint cycle is a cold clippy run, 31–45 s).

## Results

| | default | unpacked | packed |
| --- | ---: | ---: | ---: |
| Cold `cargo build`: time | 84.7 s | 76.3 s | 80.9 s |
| Cold `cargo build`: `target/` | 5.0 GiB | 2.7 GiB | 2.8 GiB |
| Cold `just test`: time | 115.4 s | 112.1 s | 109.5 s |
| Cold `just test`: `target/` | 6.4 GiB | 3.6 GiB | 4.1 GiB |
| Warm mdoc edit → `cargo test -p mdoc --no-run` | 3.4 s | 3.8 s | 6.4 s |
| Warm editor edit → `cargo test -p mdoc --no-run` | 4.3 s | 4.1 s | 6.7 s |
| Warm editor edit → `just lint` | 2.2 s | 1.9 s | 1.9 s |
| New object files per mdoc / editor cycle | 742 / 972 | 742 / 972 | 0 / 0 |
| Bytes per warm build cycle | ≈ 0 | ≈ 0 | ≈ 0 |
| Bytes per warm lint cycle | 34 MiB | 34 MiB | 34 MiB |
| `target/` after all steps | 7.6 GiB | 4.6 GiB | 5.1 GiB |

## Findings

- Line tables with no dependency debug information cut every complete build
  configuration by about 45 % (5.0 → 2.7 GiB built, 6.4 → 3.6 GiB tested) at no
  time cost. Backtraces keep file and line (checked with a panicking test).
- `packed` stops object files accumulating but adds about 2.6 s to every warm
  test build (+68 %), beyond the agreed 10 % / 2 s limit. `unpacked` stays.
- Under `unpacked`, each warm cycle leaves ~750–1,000 new `*.rcgu.o` names, but
  unchanged codegen units are hardlinks to the same incremental objects, so a
  small edit costs almost no bytes. File counts overstate disk use: the
  2026-10-16 main `target/` held ≈202 GiB of apparent object size on 135 GiB
  of disk.
- Growth from edits alone cannot explain the 150 GiB main `target/`. The
  remaining driver is artifact-hash churn: a toolchain bump, dependency update,
  version bump or changed flags compiles a complete new copy under new hashes,
  and Cargo never removes the old copy. The old `target/` held 104 `mdoc` and 37
  `docx_worker` test-binary hashes. Line tables make each copy ~45 % smaller;
  only `cargo clean --profile dev` removes stale copies.
- Clippy adds ~34 MiB per lint cycle in every configuration; debug settings do
  not affect it.

Measured afterwards, with nothing else built: retiring three worktrees
(`just retire-worktree`) removed 11, 9.7 and 5.1 GiB; free disk space went from
23 GiB (before cleaning the 150 GiB main `target/`) to 192 GiB.
