# CI on a merge queue

Status: Accepted, 2026-11-02.

## Why

Local CI on the dev Mac and the Windows PC tested every PR head. On busy
days a Mac PR waited 45 to 60 minutes, every new head re-ran everything,
results named a `main` hours old, and the hosted fallback posted green
statuses that had tested nothing. GitHub's merge queue on `main` runs a fast
tier on each head and the full suite of the touched areas once, on `main`
plus the group, right before the merge, so no PR merges without a full
result. The ruleset requires only `lint (ubuntu)` and the aggregate
`merge-gate`, posted by a workflow that never checks out PR code, so moving
a job never touches the ruleset. A required status is `success` only when
its job ran in full. The Mac is not a runner, and third-party code never
runs on the PC. CI uses only free hosted runners and the existing machines.

## Rejected

- **Testing every PR head in full**: the waits and stale results above.
- **Retrying flaky tests**: a flaky test goes into a dated quarantine list
  (hosted iOS UI tests excepted).
- **Paid runners**.

## Revisit when

GitHub's free macOS limit (5 concurrent jobs) stalls the queue, or the PC
is gone.
