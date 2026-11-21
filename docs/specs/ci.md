# CI

How a PR reaches `main`. The machinery and the PC's runners:
[scripts/ci/README.md](../../scripts/ci/README.md).

## Merging

- `main` merges only through GitHub's merge queue; a ruleset requires
  `lint (ubuntu)` and `merge-gate` on the PR head and on the queue commit.
- Never merge with `--admin`. The maintainer's bypass is for `just release`
  and break-glass reverts only.
- Nothing posts a waiver or a partial result as green.
- Enqueue a green, reviewed PR with `gh pr merge <N> --auto`, and
  independent PRs off `main` together so they batch. The queue takes only
  PRs based on `main`, so a stack goes one PR at a time.
- Merge `main` into a PR only when GitHub reports it `CONFLICTING`, because
  each new head reruns its checks.

## A red or stale gate

- If `merge-gate` says "Waiting" after its checks finished, or a green PR
  with auto-merge is not queued after 5 minutes, run
  `gh workflow run merge-gate.yml --repo lawkitt/mdoc`; never re-run the
  PR's own `pull_request_target` gate run.
- A queue failure is unrelated when it is infrastructure, the same test
  failed the same way on another queue commit, on `main` or in the nightly
  within 14 days, or the failing test is in an area the PR does not touch.
  Anything else goes back to the PR's author.
- After an unrelated failure the hub re-queues the PR once, naming the
  matching rule in a PR comment. A second drop goes to the maintainer.
- A red `main` stops enqueueing until fixed.

## Quarantine

- A pass on the hosted iOS UI tests' one retry counts as an unrelated
  failure.
- After a test's second unrelated failure, the hub quarantines it in
  `.config/quarantine.txt` (its header has the format) by PR, without
  asking.
- Tests that measure speed, take screenshots or survey layouts stay off the
  queue.

## Detection snapshots

- Hosted `windows-latest` is the reference platform, because a developer
  machine's scores differ in the fourth decimal.
- Never commit your own `--refresh-snapshots` output: push, let the head's
  detection gate fail, run `just snapshots-fetch <PR>`, review the diff,
  then commit and push.
- After a runtime or model pin change, record every snapshot with
  `gh workflow run ci-detection.yml --ref <branch> -f record=true`, because
  scores can shift within the gate's tolerance.

## Changing CI

- A change to what decides merges (`merge-gate.sh`, `queue.yml`, the job
  workflows, `queue-runner.sh`, `ci-areas.txt`) first passes a shadow run on
  `ci-shadow/main/<name>`, with a deliberately broken twin that must fail.
  Delete both branches afterwards.
- If a merged gate change blocks the queue, the maintainer pushes its revert
  straight to `main`.

## iOS tests on the Mac

- Run only the test classes you touched
  (`just ios-test "iPhone 17" mdocUITests/<Class>`), one heavy build at a
  time after checking `uptime`, because the full suite takes 25 to 30
  minutes on the fanless Air and parallel runs overheat it.
