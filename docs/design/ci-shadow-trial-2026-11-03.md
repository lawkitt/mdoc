# CI shadow trial, 2026-11-03

ROADMAP CI redesign item 5 ([ADR 0095](../adr/0095-ci-on-a-merge-queue.md)),
run by the CI redesign hub before the cutover. The maintainer chose the
shorter path: two PRs and deliberately broken branches instead of five PRs,
and the cutover with Android on the PC only.

Each trial branch was one commit on `main` under `ci-shadow/main/`, built
the way the merge queue builds a group, so `queue.yml`, the PC's runners and
`merge-gate` treated it as a queue commit. Nothing merged, and no PR branch
or PR status changed.

## Wave 1: `main` at 8c0ee57, the Mac's snapshots

| Branch | Needs | Result | As expected |
|---|---|---|---|
| #527 replayed | macos, windows, detection | macos passed; windows passed (hosted fallback, PC off); detection failed | yes: the Mac's snapshots |
| #528 replayed | macos, windows, detection | macos passed; windows failed on the PC (test bug); detection failed | detection yes; windows no |
| broken Rust (a failing `mdoc-pii` test) | macos, windows, detection | macos and windows (hosted at once) failed on that test | yes |
| broken iOS (a failing XCTest) | ios | ios failed; the xcresult names only `TextOffsetsTests/testASCIIOffsetsAreEqual()` | yes |

Paths this exercised: jobs from areas (the iOS-only branch ran only
`ci/ios`), the hosted Windows fallback after the PC was silent for 10
minutes, a group with no PR number taken as third-party (hosted Windows at
once), and `merge-gate` failing as soon as one job failed.

## Snapshots

The broken Rust branch is `main` plus one test file, so its failing
detection shards uploaded `main`'s snapshots regenerated on hosted:
132 files, scores only (at most 0.00193 apart from the M4's), every entity
the same, as the lane 2d experiment found. They are #534, which merges at
the cutover.

## Found and fixed

- `queue-runner-setup.ps1` did not parse (`"$outArg:"`), #533.
- `test-queue-runner.sh` read the PC's real `runner.env` through a literal
  `/e/ci`, #533.
- On Windows a stopped job could start its next step (`taskkill` order),
  #536; 3 of 3 suite runs on the PC passed after it.
- After the PC restarted, `tailscale serve` had lost `/ci`; restored by
  hand, and a ROADMAP item 7 follow-up.
- PRs that change only docs never ran `lint (ubuntu)`, which the ruleset
  requires; #535 drops `ci.yml`'s PR path filter.

## Wave 2: `main` + the PR + snapshots fetched from its wave 1 run

The step a PR author takes after the cutover: `just snapshots-fetch <run>`.

| Branch | Result |
|---|---|
| #528 | **passed: macos (hosted), windows (PC), detection (hosted)** |
| #527 | windows failed on the PC on the `taskkill` race (#536); rerun as wave 3 on `main` with #536 |

The current gate (local CI) passed both PRs' heads, so the two setups agree
wherever the new one ran in full.
