# CI on a merge queue

Status: Accepted, 2026-11-02.

## Problem

Local CI on the dev Mac and the Windows PC tested every PR head. On a busy
day the Mac runner, sharing the machine with agent builds, stalled GPUI
tests, a Mac PR waited 45 to 60 minutes, every new head re-ran everything,
results named a `main` hours old, and the hosted fallback posted green
statuses that had tested nothing.

## Decision

- **GitHub's merge queue on `main`**, by a ruleset that requires
  `lint (ubuntu)` and `merge-gate`, with a bypass for the maintainer's
  direct pushes (`just release`, break-glass reverts). Agents enqueue with
  `gh pr merge --auto` and never use `--admin`.
- **Two stages.** A PR head runs a fast tier on hosted Linux. The queue
  commit (`main` plus its group) runs the full suite of the areas the group
  touches, once, right before the merge.
- **`merge-gate`** is one aggregate status, posted by a hosted workflow
  that runs `main`'s script and never checks out PR code. The per-job
  statuses are shown, not required, so moving or adding a job never touches
  the ruleset.
- **A required status is `success` only when its job ran in full.**
  Decisions from the diff ("Not affected", "Docs only") stay green; a job
  with nowhere to run stays pending.
- **Where jobs run:** hosted runners for macOS, iOS, detection and the fast
  tier; the Windows PC for the Windows desktop and Android jobs, queue
  commits only, with hosted fallbacks. The Mac is not a CI runner.
- **Third-party code** (fork and bot PRs) never runs on the PC; their queue
  groups run hosted.
- **Tests never call OS security or identity services**; a guard test fails
  when a test binary reaches one.
- **Tests never retry** (hosted iOS UI tests excepted); a flaky test goes
  into a dated quarantine list.
- **Free only:** hosted runners and the existing machines.

## Consequences

- PR heads give feedback in minutes; the full suite runs once per queue
  group, and a break found there drops only the failing PR.
- macOS testing depends on GitHub's hosted runners (5 concurrent macOS jobs
  on the Free plan); Windows and Android depend on the PC, with hosted
  fallbacks.
- No PR merges without a full result for every affected area on `main` plus
  that PR.
- The current rules: [specs/ci.md](../specs/ci.md) and
  [scripts/ci/README.md](../../scripts/ci/README.md).
