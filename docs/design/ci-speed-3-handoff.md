# CI speed, round 3: handoff for a grill (2026-10-29)

For a new Claude Desktop session in this repository that grills a significant
CI optimization. Invoke the `grilling` skill, then read this file,
[ADR 0070](../adr/0070-ci-speed-and-script-hygiene.md),
[round 2](ci-speed-2.md), [ADR 0037](../adr/0037-ci-and-release-pipeline.md),
`.github/workflows/ci.yml`, `release.yml`, `.github/actions/setup`, the
`justfile` (`lint`, `test`, `check`) and `tests/headless_renders.rs`. Facts
below are from 2026-10-29; re-measure before deciding.

## Measurements

Last 25 `ci.yml` runs (green jobs only):

| Job | Runs | Median | Max |
|---|---|---|---|
| `lint (ubuntu)` | 20 | 1.1 min | 1.6 |
| `clippy (macos-26)` | 20 | 1.5 min | 1.9 |
| `clippy (windows-latest)` | 20 | 2.4 min | 3.2 |
| `test (macos-26)` | 14 | 11.1 min | 11.9 |
| `test (windows-latest)` | 11 | 12.4 min | 15.2 |

Queueing is negligible (median 0.1 min). On a green `main` run the `just test`
step alone took 12.8 min on Windows and 9.8 min on macOS. ADR 0070's target was
about 8 minutes for Windows, round 2's 7; both are missed, and the job has grown
as 0.3.0 added screens (41 entries in `tests/headless_renders.rs`'s list; the
render test took 240 s locally on an M4).

- **Cache:** 17 caches, 11.6 GB, over GitHub's 10 GB repository limit, so
  eviction is live. `main` holds several generations per job (keys differ only
  in the trailing hash, e.g. `…-57c00e6c`, `…-ce98b0bd`, `…-796622c3`):
  about 1.1 GB per Windows test cache, 0.8–0.9 GB per macOS test cache.
- **Artifacts:** each run uploads headless renders, 249 MB (macOS) and 153 MB
  (Windows), kept 14 days. The hub downloads ~450 MB to look at one light and
  one dark screen of a PR, which took about 5 minutes per PR.
- **No job timeout:** on 2026-10-29 PR #215's test jobs hung for over an hour
  on both OSes (a test-level deadlock after a merge) before the hub cancelled
  them; GitHub's default job timeout is 6 hours.
- **`cargo test`** runs test binaries one after another; round 2 chose not to
  adopt cargo-nextest "for now".
- Hosted Windows runners are bimodal in speed (round 2: render ~230 s vs
  ~430 s); timings must be compared on the same CPU model, which the test
  jobs log.

## Parallel work makes CI the bottleneck

On 2026-10-29 up to six PRs were open at once. Each push re-ran the full
matrix; five merges of `main` into PR branches (conflicts in `ROADMAP.md`,
`src/workspace/headless_renders.rs`, `tests/headless_renders.rs`,
`src/workspace/cases_ui.rs`, `crates/mdoc-pii/src/restore_tests.rs`) each
cost another full run. The two shared render files conflict on almost every
UI PR.

## Questions for the grill

1. **Targets:** what wall time should a PR's CI aim for (per OS), warm and
   cold, and is the render test still on every PR?
2. **Job timeouts:** `timeout-minutes` per job (for example 20 for tests),
   and a per-test timeout so one deadlock names itself instead of hanging.
3. **Cache:** fix the generations on `main` (one key per job, or delete old
   generations after a save), `cargo-sweep` or `CARGO_INCREMENTAL=0` to shrink
   caches, and whether `sccache` (ROADMAP Engineering already lists it) helps
   on hosted runners.
4. **Test runner:** cargo-nextest now (parallel binaries, per-test timeouts,
   retries, JUnit output), or keep `cargo test`.
5. **Render test:** shard it across processes or jobs; render only screens a
   PR touches on PRs and all on `main`; fewer Tab-walk PNGs; smaller PNGs.
6. **Artifacts:** a small per-PR artifact (changed screens, or a contact
   sheet of light and dark) besides or instead of the full set; shorter
   retention.
7. **Skipping work:** path filters per job (docs-only already skip), skipping
   Windows clippy when no Windows-cfg code changed, or running the heavy
   tests only when a PR is ready rather than on every draft push.
8. **Conflicts:** split `headless_renders.rs` scenes and the screen list into
   per-feature files so parallel UI PRs stop conflicting; a merge queue (GitHub
   merge queue or hub-driven order) so `main` merges don't trigger N re-runs.
9. **Runners:** ADR 0070 keeps free hosted runners only. Does that still hold
   (larger runners cost money; self-hosted on the PC raises security for a
   public repo)?
10. **Release pipeline:** whether `release.yml` (which calls `ci.yml`) needs
    the same changes.

Record decisions as a new ADR that amends ADR 0070, with the interview in this
folder; the changes then go out as their own PRs.
