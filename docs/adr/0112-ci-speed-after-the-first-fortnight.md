# CI speed after the queue's first fortnight

Status: Accepted, 2026-11-12. Amends [ADR 0095](0095-ci-on-a-merge-queue.md)
(the Windows job's steps and limit, the iOS job's shape, flakes) and
[ADR 0100](0100-ci-reliability-after-the-cutover.md) (`ci/ios`, the
targets, measuring them). Rolled out by ROADMAP's CI speed section.

## Problem

On 2026-11-11/12 the queue merged about 20 PRs, and most drops were not
caused by the PR they hit ([grill](../design/ci-speed-grill-2026-11-12.md)):

- The PC's `ci/windows` passed 3 times in about 18 runs. In every failed
  run that got through its tests, all Rust tests passed: the script suites
  `just test` runs beside them failed or ran out the 45-minute limit. They
  took about 25 of the job's 30 to 45 minutes, and their thousands of
  process starts slowed the Rust tests beside them.
- Hosted `ci/ios (tests)` took 31 to 46 minutes and was twice cancelled at
  its limit. 54 UI tests took 84% of the time inside tests; two of them
  only measure typing speed (244 s together).
- A known Android flake was not quarantined, and every re-queue (about ten
  a day) waited for the maintainer.
- Building is not the bottleneck (2 to 4 minutes of each job), so build
  tools (gpui-fast, a Cargo fork, sccache, other linkers, Cranelift) would
  not help.

## Decision

### Jobs

- **`ci/windows` runs the Rust tests and doctests only** (the tiers, not
  `just test`). The script suites stay on hosted Linux in every PR head's
  fast tier, and under macOS bash 3.2 for `ci`-area changes; a script
  change never runs the Windows job.
- **Its limit is 30 minutes**, `queue-runner.sh`'s default: the PC's
  45-minute override goes.
- **The PC stays the main Windows runner**, hosted Windows its fallback.
  If, in the week after the change, the PC still causes more than one
  queue drop, hosted Windows becomes the main runner. Defender exclusions
  and a run with the VPN off come only if the Rust tests still pass 12
  minutes more than once in that week.
- **`ci/ios` is the summary of two parallel jobs**: `ci/ios (unit)`, the
  unit and parity tests followed by the unsigned Release build for the
  Simulator (which replaces `ci/ios-release`), and `ci/ios (ui)`, the UI
  tests. A second UI job, sharded by test class, only if the UI job stays
  over 20 minutes.
- **Timing tests run nightly**, never in the queue. Tests that only take
  screenshots or survey layouts stay off the queue as they are now.
- Areas are unchanged: a core change runs every job, and every iOS change
  runs both iOS jobs.

### Tests

- **What a test must earn.** A test is deleted when it covers removed
  behaviour or when a lower-level test asserts the same thing. A UI test
  covers what only the real UI shows: gestures, layout, the keyboard,
  system behaviour. A slow unit test is a bug to fix.
- **A sweep applies this once**, one PR per suite, iOS first: the iOS UI
  tests and slow iOS unit tests; Rust tests over 10 s on hosted macOS; the
  script suites over 60 s on Linux; the Android instrumented tests.
- **Agents on the Mac** run only the iOS test classes they touched
  (`-only-testing`) and leave the full suite to the queue; one heavy build
  at a time, `uptime` first.

### Flakes and re-queues

- **An unrelated failure** is any of: an infrastructure failure (ADR 0100);
  a test that failed the same way on another PR's queue commit, on `main`
  or in the nightly within 14 days; a failing test in an area the PR does
  not touch. Anything else is related and goes to the PR's lane.
- **The hub acts without asking**: after a test's second unrelated failure
  it quarantines it and opens the issue, and it re-queues a PR once after
  an unrelated failure, naming the rule that matched in a PR comment. A
  second drop of the same PR goes to the maintainer.

### Targets

- Replacing ADR 0100's: a queue commit's median time to a green gate 25
  minutes or less; `ci/windows` 15 minutes or less; `ci/ios (ui)` 20
  minutes or less; at most one queue drop a week not caused by the PR.
- **The nightly runs `scripts/ci-stats.sh`** with these targets and opens
  an issue after 3 red nights.

### Build tools

- None now. gpui-fast speeds up the running app, not builds; it goes to
  ROADMAP's Later list as an app-performance idea. huacnlee/cargo helps
  only repeat builds on one machine, ignores `CARGO_TARGET_DIR`, and is two
  weeks old with one maintainer. Build tools are reconsidered, with
  `cargo build --timings`, when a job's warm build step passes 5 minutes.

## Invariants

ADR 0095's and ADR 0100's invariants hold. Also:

- Every script suite still runs on every change that could break it, on
  hosted Linux at least.
- A test that measures speed never decides a merge.
- The hub never re-queues the same PR twice without the maintainer.

## Consequences

- `ci/windows` should take about 14 minutes instead of 30 to 45, so the PC
  stops being the queue's bottleneck.
- A queue commit that touches iOS waits about 25 minutes for the UI job
  instead of 31 to 46; iOS keeps two macOS runners.
- The script suites no longer run on Windows Git Bash in CI. A
  Windows-only break in `queue-runner.sh` shows on the PC itself, as the
  runner's own failure.
- The maintainer decides only second drops and real failures; the hub's
  quarantine lines are visible in PRs and expire as before.

Terms: [glossary](../../CONTEXT.md). References:
[interview](../design/ci-speed-grill-2026-11-12.md).
