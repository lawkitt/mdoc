# CI speed, round 2 (interview, 2026-10-27)

Amends [ADR 0070](../adr/0070-ci-speed-and-script-hygiene.md) once settled.

## Measurements

17 green `ci.yml` runs on 2026-10-26/27, `test (windows-latest)`:

- `just test` step about 10m40s: ~38 s freshness checks, ~2m10s warm test
  build, 47 s `mdoc` unit tests, `tests/headless_renders.rs` ~430 s.
- Render time is bimodal: ~230 s (3 runs) or ~430 s (14 runs), with the warm
  build 1m15–1m40 vs 2m00–2m27. Hosted Windows runners vary in hardware.
- macOS: render test 218 s, test build 63 s.
- One render run: 23 screens × 2 themes at 2200×1520, a PNG per Tab step
  (`CompressionType::Fast`), WARP software rasterization, workspace and
  dependencies unoptimized except the PDF crates.
- `cargo test` runs test binaries serially, so the other binaries wait.
- Cold Mac test build on `main` (cf6a805, M4, warm registry, empty
  `CARGO_TARGET_DIR`, `cargo test --locked --workspace --features
  headless-renders --no-run`): 79.7 s real. The ≤30% rule allows ~104 s.
  The same on `windows/render-speed` (#197, c816f30): 78.4 s (no growth).

## Decisions

1. **Scope of this batch:** CI concurrency, the uninstall-keeps-case-key
   check, and the Windows test-job speed work. The update check before
   publishing and changelog fragments get their own grills later.
2. **Concurrency:** `cancel-in-progress` for pushes and PRs alike, and
   `pull_request: closed` also triggers a run (jobs skip on `closed`) so a
   merged or closed PR's run is cancelled. `workflow_call` from release
   keeps `cancel-in-progress: false`.
3. **Target:** `test (windows-latest)` finishes in 7 minutes or less warm on
   the slow (~430 s render) runners; the render test stays on every PR.
4. **Levers, in order:** (1) per-crate `opt-level` for crates a profile of
   the render run shows hot, under ADR 0070's ≤30% cold-build rule;
   (2) sharding the render run across processes if the profile shows idle
   cores; (3) a separate render job only if (1)+(2) miss the target. No
   cargo-nextest for now. The render test's evidence is not reduced.
5. **Runner CPU:** the test jobs log the runner's CPU model so timings are
   compared on like hardware.
6. **Records:** amend ADR 0070; this file is the interview record.

## Delegation

7. **Split.** A, render speed (profile, per-crate `opt-level`, sharding if
   the rule applies, runner CPU log): Codex on `windows-lane-1`, branch
   `windows/render-speed`. B, concurrency and the uninstall-keeps-case-key
   check: Codex on `windows-lane-2`, branch
   `windows/ci-concurrency-key-check`. C, ADR 0070 amendment and ROADMAP:
   Claude on the hub, after A's numbers.
8. **Cold Mac build** for the ≤30% rule: measured on the hub with a separate
   `CARGO_TARGET_DIR`, `main` against A's branch, after `df -h`.
9. **Uninstall check proof:** B runs a `workflow_dispatch` Release test build
   from its branch (`checks: false`, `sign: false`) and links the passing
   `installed-windows-desktop` run in its PR.
10. **A's autonomy:** profile → `opt-level` → sharding without stopping;
    stop for the maintainer only if a separate render job would be needed.
11. **Order:** merge B, then A (rebased), then C. Claude reviews A and B on
    the hub; the maintainer merges.
12. **Follow-up:** no away-mode alerts; the maintainer asks the hub to check.

## After the first results (2026-10-28)

13. **#197 shape (option b).** Keep the per-crate `opt-level` 2 for
    `gpui-pre-windows`, `png` and `simd-adler32` and the runner CPU log;
    replace the PNG encoder child processes with a background encoder thread
    in the render process. On 4-vCPU runners WARP rasterization is the limit,
    so the target becomes about 8 minutes warm on slow runners instead of 7,
    and no separate render job is added.
