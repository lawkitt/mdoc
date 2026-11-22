# CI

How a PR reaches `main`. The machinery and the PC's runners:
[scripts/ci/README.md](../../scripts/ci/README.md).

## Merging

- `main` merges only through GitHub's merge queue, which requires
  `lint (ubuntu)` and `merge-gate` on PR heads and queue commits.
- Never merge with `--admin`; the maintainer bypasses only for
  `just release` and break-glass reverts.
- Nothing posts a waiver or partial result as green.
- Enqueue a green, reviewed PR with `gh pr merge <N> --auto`, and
  independent PRs off `main` together so they batch.
- Merge `main` into a PR only when GitHub reports `CONFLICTING`, because
  each new head reruns its checks.

## A red or stale gate

- If `merge-gate` says "Waiting" after its checks finished, or a green PR
  with auto-merge is not queued after 5 minutes, run
  `gh workflow run merge-gate.yml`; never re-run the PR's
  `pull_request_target` gate run.
- A queue failure is unrelated when it is infrastructure, the same test
  failed the same way on another queue commit, on `main` or in the nightly
  within 14 days, or the failing test is outside the PR's areas. Anything
  else goes back to the PR's author.
- After an unrelated failure the hub re-queues the PR once, naming the
  rule in a comment. A second drop goes to the maintainer.
- A red `main` stops enqueueing until fixed.

## Quarantine

- A pass on the hosted iOS UI tests' one retry counts as an unrelated
  failure. A UI part whose runner failed before any test reruns once, warned,
  as infrastructure.
- After a test's second unrelated failure, the hub quarantines it in
  `.config/quarantine.txt` by PR, without asking.
- Tests that measure speed, take screenshots or survey layouts stay off the
  queue.

## Detection snapshots

- Detection's model budget fails only the nightly, because a shared VM's
  wall clock must not decide merges.
- Never commit your own `--refresh-snapshots` output, because a developer
  machine's scores differ from hosted `windows-latest`'s: commit what
  `just snapshots-fetch <PR>` takes from the head's failed gate, reviewed.
- After a runtime or model pin change, record every snapshot
  (`ci-detection.yml`'s `record`), because scores shift within the gate's
  tolerance.

## Changing CI

- A change to what decides merges (`merge-gate.sh`, `queue.yml`, the job
  workflows, `queue-runner.sh`, `ci-areas.txt`) first passes a shadow run on
  `ci-shadow/main/<name>`, with a deliberately broken twin that must fail.
  Delete both branches afterwards.
- If a merged gate change blocks the queue, the maintainer pushes its revert
  straight to `main`.

## iOS tests

- On the Mac, test only the classes you touched
  (`just ios-test "iPhone 17" mdocUITests/<Class>`), one build at a time
  after checking `uptime`: the full suite takes 25–30 minutes and parallel
  runs overheat the Air.
- The queue tests on Xcode and iOS 26.2, the nightly on 27.2 (ADR 0124); a
  test passes on both. Reproduce a queue failure with Xcode 27.2 on the
  iOS 26.2 runtime, because Xcode 26.2's runner hangs on macOS 27.
- Branch `ci-ios.yml` dispatches test what they touched, one at a time,
  because they took 13% of macOS minutes.
