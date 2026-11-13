# CI targets by commit, and detection off the macOS runners

Status: Accepted, 2026-11-13. Amends
[ADR 0112](0112-ci-speed-after-the-first-fortnight.md) (the targets, the
iOS UI job, flakes and re-queues) and
[ADR 0095](0095-ci-on-a-merge-queue.md) (where the detection gate runs,
its reference platform, tests never retrying). Rolled out by ROADMAP's CI
speed section, items 23 onwards.

## Problem

A day after ADR 0112's rollout its targets mostly pass (queue median 20.3
minutes, `ci/windows` 6–7, no drops not caused by the PR), but the median
hides two kinds of commit ([grill](../design/ci-grill-2-2026-11-13.md)):

- **A core change takes 33–37 minutes**, a desktop-only change about 14.
  A core change spends 90–120 minutes of hosted macOS time in 11 jobs, on
  the Free plan's 5 macOS slots: at least 18–25 minutes however well they
  pack. Six of those jobs are the detection gate's shards, which waited
  7–28 minutes for a runner.
- **The detection gate runs for every desktop change**, though it builds
  only `mdoc-detect` and the crates it depends on.
- **The two iOS UI shards were unbalanced** (8 and 18 minutes of tests).
- **Hosted iOS failures name no test**, so ADR 0112's "same test
  elsewhere" rule and `ci-stats.sh` cannot see an iOS flake: every hosted
  iOS failure counts as caused by the PR. Five UI tests failed once each
  in two weeks; the one that dropped #836 waited 5 s for a state the app
  had reached while one accessibility query took 4.2 s.
- **Most unit-tier time is one fixture**: nearly every `mdoc-detect` unit
  test builds the rule pack again (about 737 s of the hosted macOS unit
  tier's 1,504 s of CPU time).
- **UI test time is mostly launches** that import a seed document (15–20 s
  each), not test steps.

## Decision

### Targets (replacing ADR 0112's)

- **A core change's queue time median: 20 minutes or less.**
- **A desktop-only change's: 15 minutes or less.**
- **Every queue commit's: 25 minutes or less**, ADR 0112's median, kept as
  a floor.
- **The slower iOS UI shard's median: 15 minutes or less**, replacing
  `ci/ios (ui)` 20.
- At most one queue drop a week not caused by the PR, as before.
- **`scripts/ci-stats.sh` classifies each queue commit** as core,
  desktop-only, iOS-only or other by the jobs it ran, and reports each
  target. The nightly's issue after 3 red nights judges only data from
  after the change a target depends on. The targets and the check land in
  one PR.

### The detection gate

- **It runs only when its inputs change**: the core area, the recognition
  fixtures, `resources/`, and the crates the gate builds. App-only files
  (`src/`, `vendor/`, `build/`, `tests/` outside the recognition fixtures)
  leave its area, so a desktop-only change runs `ci/macos` alone. Its
  core trigger stays: narrowing it to the gate's crates would skip about
  one core commit in nine.
- **It moves to hosted `windows-latest`**, whose 20-job pool it shares
  with Linux, not the 5 macOS slots. **Windows becomes its reference
  platform**: the 256 snapshots are recorded there once, and the Mac takes
  refreshed snapshots from CI's `detection-snapshots-<k>` artifacts
  (`just snapshots-fetch`). Linux is not an option: mdoc's pinned ONNX
  Runtime supports only macOS arm64 and Windows x86_64.
- **A trial comes first**: one dispatch on a branch measures the time per
  fixture and how many snapshots change, and sets the shard count. If the
  FP16 model cannot fit its 5-minute budget in 12 shards, the gate stays on
  macOS and this decision is reopened.

### iOS tests

- **The UI shards split by measured time**: a committed table of test
  class times from recent xcresults, read by the shard script and
  refreshed by the nightly. No build-once hand-off between jobs: it saves
  about 3 runner-minutes and puts a serial build on the critical path.
- **A failed iOS job names its failing tests**: `ci-ios.yml` reads the
  xcresult and writes them into the job's error and summary, as the PC's
  status does.
- **Hosted iOS UI tests retry once** (`-retry-tests-on-failure
  -test-iterations 2`). A pass on the retry keeps the job green, is named
  in the job's summary, and counts as an unrelated failure toward ADR
  0112's quarantine after two. Unit, Rust, Android and script tests still
  never retry.
- **UI tests share one timeout for waits on a state to appear** (about
  15 s), replacing the 5 s waits. Such a wait returns as soon as its
  condition holds; waits for something to go away keep their own, shorter
  timeouts.
- **UI tests launch from a prebuilt library**, a fixture copied into the
  app's container instead of an import at each launch: tried on the class
  with the most launches first, then every class if it saves at least 5 s
  a launch.

### Unit tests

- **The `mdoc-detect` rule pack is built once per test process**, shared
  and built lazily; it is immutable once built.

### Unchanged

- ADR 0112's rule for unrelated failures stays mechanical: no clause for
  "the recording shows the app got there".
- ROADMAP CI speed item 22 (the PC's slow bursts) closes as not needed:
  since item 16 the Windows job takes 6–7 minutes.

## Considered and not chosen

- **One overall median of 15 minutes**: a day of docs PRs passes it while
  every core change takes 35.
- **A core change in 15 minutes**: the UI shards alone take about 13.
- **Detection on the PC**: faster and idle, but it brings back the sleep
  and offline problems and ADR 0095's fallback for one more job.
- **Larger or more macOS runners**: more than 5 macOS slots needs GitHub
  Enterprise or paid larger runners.
- **Retries everywhere**: unit, Rust and Android tests have no Simulator
  timing noise, and a retry would hide real races there.

## Invariants

ADR 0095's, ADR 0100's and ADR 0112's invariants hold, except that hosted
iOS UI tests retry once. Also:

- A retried pass is always reported by name and counts toward quarantine;
  a retry never hides a test silently.
- The detection gate checks snapshots on the platform that recorded them.
