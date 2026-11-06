# CI reliability after the cutover

Status: Accepted, 2026-11-06; amended 2026-11-05 at the rollout (below). Amends [ADR 0095](0095-ci-on-a-merge-queue.md)
(what a status means, the gate's passes, `main`'s runs, the jobs and their
shape, areas, logs, upkeep). Rolled out by ROADMAP's CI redesign items 8–15.

## Problem

In the three days after the merge-queue cutover (2026-11-03 to 11-05), CI
slowed iteration a lot ([grill](../design/ci-fixes-grill-2026-11-06.md),
issue #614):

- A queue commit took a median of 21.8 minutes to a green gate (p90 30),
  and stacked PRs went through one at a time: 7 PRs took 5 hours.
- 20 PRs were dropped from the queue. 8 of those drops were infrastructure:
  hosted macOS jobs that never got a runner counted as test failures, and
  three PRs sat out overnight.
- A `merge-gate` pass that hit a GitHub API error skipped the PR silently
  and ended green. The `*/5` schedule ran 10 times in 3 days, so nothing
  retried (#613's stuck gate).
- `main`'s hosted Windows tests were red for two days without anyone being
  told, while ADR 0095 said `main` runs cache-saving builds only.
- No job built the iOS Release configuration, so a Release-only compile
  error surfaced at the archive.
- The PC's log page was off the tailnet for a day, its Windows output kept
  about one day of logs, and its Android failures named no test.

## Decision

### What a status means

- **A job that never ran is not a result.** A hosted job that ended without
  a runner (cancelled with no runner assigned, or a summary over jobs that
  never ran) is an infrastructure failure. `merge-gate` re-runs it up to
  twice and stays pending. Only then does it fail, worded as infrastructure
  ("no macOS runner"). A completed job's failure still fails at once, and
  neutral or stale still prove nothing.

### The gate's passes

- **Every GitHub call is bounded and retried**, and names its endpoint when
  it fails. A PR head or queue commit skipped because of an error is logged,
  and the run is red. No pass ends green after skipping something.
- **Follow-up pass.** A pass that leaves anything pending, or hit an error,
  dispatches `merge-gate-recheck.yml`. That workflow runs in its own
  concurrency group (a newer recheck replaces an older one), sleeps 3
  minutes, then dispatches `merge-gate.yml`. The `*/5` schedule stays, but
  nothing relies on it.
- **Missed enqueue.** The gate warns, with a log line and an annotation,
  when an open PR has auto-merge on, both required checks green, and no
  queue entry after 5 minutes. Re-check the gate with
  `gh workflow run merge-gate.yml`, never by re-running a PR's
  `pull_request_target` run. The gate gets no new permissions.
- The gate reads the queue commit's own area table, as `queue.yml` and the
  PC do.

### `main`

- **`main`'s pushes run lint and `--no-run` builds** on macOS and Windows,
  as ADR 0095 said. They include the iOS and Android targets that
  `just mobile --ffi` checks, so those deps are in main's cache. The nightly
  stays the full hosted check that opens an issue.

### Jobs

- **`ci/macos`** is two parallel jobs plus a summary job of that name.
  Job A: lint, `just mobile --ffi` and the bash 3.2 suites. Job B: the
  unit, recognition, app and renders tiers.
- **`ci/ios`** is the summary of the Simulator tests and `ci/ios-release`,
  a parallel unsigned Release build for the Simulator. The tests use the
  image's Simulator runtime when its major.minor matches the pin, and
  download only on a mismatch. The core for the tests stays in the `mobile`
  profile. Parallel testing on 2 Simulator clones is a trial, kept only if
  faster and stable.
- **The detection gate runs 3 shards** (the model run's 5-minute budget
  rules out 2). A shard warns past 80% of its budget, the signal to add a
  shard. Its failure message prints scores to enough digits to show the
  difference.
- **The fast tier is two parallel jobs**, unit and scripts, under a summary
  job named `fast (ubuntu)`.
- **Recognition-area PR heads run the detection gate**, and the head
  `merge-gate` waits for it. Lanes stop dispatching it by hand.
- **The Android job builds the core for x86_64 and the host only** in CI.
  aarch64 stays compile-checked by `just mobile --ffi` and is built for
  devices and releases.
- Linux package installs have a 5-minute limit and one retry.
- The queue keeps building 2 groups at once. A dedicated CI Mac is
  reconsidered only if capacity re-runs still fail more than about once a
  week.

### Areas

- **A `ci` area**: `scripts/merge-gate.sh`, `scripts/queue-runner.sh`, the
  workflows and the CI test suites. It runs lint, the scripts tier and the
  bash 3.2 suites, not the desktop suite. A change to the queue machinery
  also gets a shadow-branch run. Recognition code keeps its desktop jobs.

### The PC

- **Logs stay on the tailnet** (ADR 0095's "No external log store" stands),
  and the page is kept up. The web task re-applies `tailscale serve` for
  `/ci` at start, and `queue-runner.sh status` and the fleet preflight check
  the tailnet URL, not only the local file.
- A Windows job keeps its renders only when it fails, so the output cap
  holds 7 days of logs.
- Statuses name failing tests, Android's from Gradle's JUnit XML. A failed
  step's last lines go into `summary.txt`.
- The two PC jobs keep running side by side. A recurring flake goes into
  quarantine. The hosted Android job (ROADMAP CI redesign item 2) comes after
  this ADR's other items, as the PC's fallback. Whether it becomes Android's
  main runner is decided with its drill's timings.

### How PRs land

- **Lanes make independent PRs off `main`** where they can, enqueued
  together so they batch. Stacks are kept for changes that truly depend on
  each other.
- **Break-glass.** A merged gate change that blocks the queue also blocks
  its own revert. The maintainer then reverts it with a direct push to
  `main` through the ruleset's existing bypass, its only use besides
  `just release`. Every PR that changes what decides merges first passes a
  shadow-branch run that includes a deliberately broken case it must fail.

### Targets

- Queue commit to a green gate: median 13 minutes or less. Fast tier: median
  4 minutes or less. At most 1 infrastructure-caused queue drop a week.
- `scripts/ci-stats.sh` measures them, by hand at first. Later the nightly
  runs it and opens an issue after 3 red days.

## Invariants

ADR 0095's invariants hold. Also:

- A pass that could not read something never posts or keeps a status as if
  it had.
- An infrastructure failure never drops a PR from the queue without first
  being re-run, and never reads as a test failure.

## Consequences

- With the cache, split and runtime changes, a queue commit should be green
  in about 11 minutes. `ci/ios` (about 10–12 minutes) becomes the longest
  job when it runs.
- A group can want 7 macOS jobs on 5 runners. The jobs that wait (detection
  shards, the Release build) are shorter than the critical path.
- `main` no longer tests on push. A break that slips past the queue shows up
  in the nightly, at most a day late.
- Recognition PR heads take about 8 minutes longer, in exchange for not
  learning about stale snapshots in the queue.

## Amendments at the rollout (2026-11-05)

What implementing it settled (#616–#621):

- **The detection gate keeps 4 shards.** On 3 shards each model run took
  248–253 s of the 300 s budget (84%), so the 80% warning fired at once.
  The decision above ("3 shards") was the plan; 4 is what holds. Manual runs
  default to 4 too.
- **`ci/ios` takes about 18 minutes, not 10–12.** The hosted image ships no
  iOS 26.1 Simulator runtime (only 26.2, 26.4.1 and 26.5), so every run
  still downloads it. Parallel testing on 2 Simulators was twice as slow
  and failed on every trial, so it stays off. `ci/ios` (about 18 min) is
  the longest job when it runs; `ci/macos` is about 8.5 min warm. The
  ways down are a pin to the image's runtime (Xcode 26.2 and iOS 26.2) or
  a cached iOS core on `main`. Both are the maintainer's decision.
- **Pinned to Xcode 26.2 and iOS 26.2 (#630, 2026-11-06).** With no
  runtime download, `ci/ios` took about 12 minutes on its shadow run
  (tests 11.8 min, Release 5.8 min in parallel), from 17–21. The pin found
  a Find-sheet hang on iOS 26.2 (#634), fixed first. The cached iOS core
  (#633) comes on top.
- **A summary's parts are named after it** (`ci/macos (checks)`,
  `ci/ios-release`, `ci/detection (2)`, `fast (unit)`): the gate treats a
  summary's parts as its caller's jobs whose names start with the summary's
  name before ` (`, and re-runs those that never got a runner.
- **A 404 is an answer.** Counting the manual runs of a workflow that does
  not exist yet (`ci-android.yml`) is 0, not an error (#621, a regression
  of #619).
- **The PC's Android CI job builds x86_64 only**, and the hosted Windows
  test-runner suite passes again (#620).

Terms: [glossary](../../CONTEXT.md).
