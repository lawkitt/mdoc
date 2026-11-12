# CI speed grill, 2026-11-12

Interview with the maintainer on where CI time goes and what to cut, after
a day (2026-11-11/12, about 20 PRs) in which most queue drops were not
caused by the PR they hit. Decisions are recorded per round; the ADR
follows when the tree closes. Earlier rounds on CI:
[ADR 0095](../adr/0095-ci-on-a-merge-queue.md),
[ADR 0100](../adr/0100-ci-reliability-after-the-cutover.md).

## Facts found before round 1

From the last 25 Queue runs (job times from the API), the PC's
`runner.log` and job folders, and the xcresult of a failed queue iOS run
(run 43108724574, 2026-11-11 22:10Z):

- **PC `ci/windows`.** 3 passes in about 18 real runs. Every failed run
  that got through its tests had all Rust tests pass (1,920 to 1,943, in
  511 to 684 s); what failed was the script suites `just test` runs beside
  them, mostly `test-queue-runner.sh` ("the windows runner did not finish
  within 900s", "oldest commit first", the heartbeat case), or the 45-minute
  limit. Split of a run: `just lint` 0.5 min, test build 2 min, Rust tests
  8.5 to 11.5 min, script suites the rest (about 25 min). The longest Rust
  test is `headless_renders` (344 s); dozens of plain `mdoc-editor` and
  `models` unit tests take 22 to 31 s each beside it.
- **Hosted `ci/ios (tests)`.** 31 to 46 min when it ran to the end. Setup
  and build about 4 min; `xcodebuild test` 36 to 39 min. In the xcresult:
  454 cases, 1,684 s inside tests, of which 54 UI tests 1,413 s (84%) and
  400 unit tests 271 s; the other 8 or so minutes are app launches and
  Simulator overhead. Slowest: `LivePreviewUITests.testTypingRestyleTime`
  127 s and `testReturnTime` 117 s (both measure speed);
  `LivePreviewUITests` 364 s in all. Some unit tests wait for tens of
  seconds (`ReviewScrollTests`: 52, 37, 31, 27 s). The survey, explore and
  device UI tests already skip on CI (about 1 s each).
- **Other hosted jobs.** `ci/macos (tests)` 9 to 17 min, `ci/macos
  (checks)` 4 to 6, detection shards 4 to 7, `ci/ios-release` 3 to 9.
  iOS is the hosted long pole, Windows the PC's.
- **Android.** `IdentitiesTest#aLongPressKeepsAllOfAnIdentity` failed on
  at least 5 PC runs in the window and is not quarantined.
- **Build time is not the bottleneck.** The PC's warm test build is 2 min;
  the iOS core uncached 3.3 min.
- **gpui-fast** (longbridge, Apache-2.0, active team, v0.1.14 on
  2026-11-09, "experimental"): a GPUI fork that cuts the running app's
  per-frame CPU. Nothing for compile or test time. Adopting it means a
  newer Zed GPUI than our `gpui-pre` 0.3.2, other crate names (our
  `[patch.crates-io]` stops applying) and porting the three vendored
  patches.
- **huacnlee/cargo** (Apache-2.0, one maintainer, created 2026-10-28): a
  `cargo` replacement that shares dependency builds across projects and
  worktrees on one machine. No gain on fresh hosted runners; ignores
  `CARGO_TARGET_DIR`; sets `RUSTC_BOOTSTRAP=1` on stable; an open report
  that its cleanup deletes build-script output; Windows least used.

## Round 1 (all recommendations accepted)

1. **The Windows job runs only the Rust tests and doctests.** The script
   suites leave it: they already run on every PR head on hosted Linux and,
   for `ci`-area changes, under macOS bash 3.2, and a script change never
   runs the Windows job anyway. Expected job time about 14 min instead of
   30 to 45, and the main cause of Windows drops goes.
2. **The iOS job splits in two: unit and UI**, in parallel (about 7 and
   25 min). A second UI job, sharded by test class, only if the UI job
   stays over 20 min after item 3. Each extra job costs a hosted macOS
   runner (5 on the Free plan; a core change already wants 8 jobs).
3. **Timing tests run nightly, not in the queue**: the two typing-time
   measurements first. Tests that only take screenshots or survey layouts
   go nightly too (they already skip on CI); behaviour tests stay.
4. **A cleanup sweep**: one agent audits the tests and opens a PR that
   deletes or shortens them, with a reason for each. The rule: delete a
   test that covers removed behaviour or that a lower-level test already
   asserts; a UI test must cover what only the real UI shows (gestures,
   layout, keyboard, system behaviour); a slow unit test is a bug to fix.
5. **Flakes and re-queues without the maintainer.** After a test's second
   failure unrelated to the PR, the hub quarantines it and opens the issue
   without asking, and re-queues a PR once after an infrastructure or
   quarantinable failure. A second drop of the same PR goes to the
   maintainer.
6. **gpui-fast and huacnlee/cargo are rejected for CI.** Neither speeds a
   fresh CI run. gpui-fast is a separate app-performance idea, outside this
   grill.
7. **No other build tool now** (sccache, mold/lld, Cranelift, prebuilt
   GPUI). Revisit with `cargo build --timings` when a job's warm build step
   passes 5 min.
8. **Agents on the Mac run only the iOS test classes they touched**
   (`-only-testing`) and leave the full suite to the queue; one heavy build
   at a time, `uptime` first. Goes into `docs/development.md`.

## Facts found before round 2

- The survey, explore and device UI tests already skip on CI, so round 1
  item 3's second half moves nothing today. The accessibility-size UI
  tests check scrolling (behaviour) and stay.
- `Install Linux build deps` takes 12 to 19 s on a normal PR run; the
  75-minute mirror outage of 2026-11-11 was one event, and #811 added the
  kernel.org HTTPS fallback.
- The PC overrides the Windows job limit to 45 min (`QUEUE_CI_TIMEOUT=2700`);
  ADR 0095 and `queue-runner.sh`'s default say 30.
- Plain unit tests taking 22 to 31 s on the PC fit CPU contention from the
  script suites, which run beside nextest and start thousands of processes.

## Round 2 (all recommendations accepted)

9. **The PC stays the main Windows runner**, hosted Windows its fallback as
   now. Re-measured for a week after round 1 item 1 lands; if the PC still
   causes more than 1 queue drop a week, hosted Windows (about 24 min
   cached, 32 cold) becomes the main runner.
10. **The Windows limit goes back to 30 min**: the PC's 45-minute override
    is removed, so `queue-runner.sh`'s default applies.
11. **The PC's slow bursts wait a week.** Only if the Rust tests still pass
    12 min more than once after item 1 lands: Defender exclusions (the
    maintainer, as admin) and one run with the VPN off.
12. **The iOS Release build folds into the iOS unit job** (about 7 + 5 =
    12 min, under the UI job), so iOS keeps 2 macOS runners.
13. **"Unrelated" failure** (for item 5) is any of: an infrastructure
    failure (ADR 0100); the same test failed on another PR's queue commit,
    on `main` or in the nightly within 14 days; a failing test in an area
    the PR does not touch. Anything else is related and goes to the lane.
    The hub names the rule that matched in its PR comment.
14. **New targets**, measured by `scripts/ci-stats.sh`: median queue commit
    to green 25 min or less; Windows job 15 min or less; iOS UI job 20 min
    or less; at most 1 queue drop a week not caused by the PR. They replace
    ADR 0100's 13-minute target.
15. **The sweep has four parts, one PR each, iOS first**: iOS UI and slow
    unit tests; Rust tests over 10 s on hosted macOS; script suites over
    60 s on Linux (the simulated runners); Android instrumented tests.
16. **No change for Linux packages.**
