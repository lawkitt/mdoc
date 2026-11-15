# CI

Folds: 0082, 0100, 0112, 0113.

How a PR reaches `main`. The machinery and the PC's runners:
[scripts/ci/README.md](../../scripts/ci/README.md).

## Required checks

- `main` merges only through GitHub's merge queue. A ruleset requires
  `lint (ubuntu)` and `merge-gate` on the PR head and on the queue commit.
- Never merge with `--admin`. The maintainer's bypass is for `just release`
  and break-glass reverts only.
- A PR head runs the fast tier on hosted Linux: `lint (ubuntu)` and
  `fast (ubuntu)` (the `unit` and `scripts` tiers). Its `merge-gate` is green
  when the fast tier passed, or when the PR is docs only.
- A recognition-area PR head also runs the detection gate
  (`detection-head / ci/detection`), and its `merge-gate` waits for it.
- A queue commit runs the full jobs its areas need; its `merge-gate` is
  green only when each ran in full and passed. Nothing posts a waiver or a
  partial result as green.
- `scripts/ci/ci-areas.txt` decides the areas. A core change runs every job,
  docs only none (fixture Markdown is not docs), and a ci-only change only
  `ci/scripts`. A new script a mobile job runs needs a line there, or it
  counts as desktop.

## Before ready

- Run development.md's gate. A full hosted job may run on your branch:
  `gh workflow run ci-desktop.yml --repo lawkitt/mdoc --ref <branch> -f os=macos`
  (for a UI change add `-f screens='<globs>'` and download
  `renders-macos-screens`), or `ci-android.yml`.

## Merging

- Enqueue a green, reviewed PR with `gh pr merge <N> --auto`. The queue
  squashes.
- A failing PR is dropped from the queue with a red `merge-gate` naming the
  failing job; fix it on the branch and enqueue it again.
- Make independent PRs off `main` and enqueue them together: the queue takes
  only PRs based on `main`, so a stack goes one PR at a time.
- Merge `main` into a PR only when GitHub reports it `CONFLICTING`:
  each new head reruns its checks.

## A red or stale gate

- If `merge-gate` says "Waiting" after its checks finished, or a green PR
  with auto-merge is not queued after 5 minutes, run `gh workflow run merge-gate.yml --repo lawkitt/mdoc`. Never
  re-run the PR's own `pull_request_target` gate run. If the PR still does
  not enter the queue, turn auto-merge off and on again.
- A queue failure is unrelated when it is an infrastructure failure, when
  the same test failed the same way on another PR's queue commit, on `main`
  or in the nightly within 14 days, or when the failing test is in an area
  the PR does not touch. Anything else goes back to the PR's author.
- After an unrelated failure the hub re-queues the PR once, naming the rule
  that matched in a PR comment. A second drop of the same PR goes to the
  maintainer.
- A red `main` stops enqueueing until fixed.

## Quarantine

- Tests never retry, except the hosted iOS UI tests, which retry once. A
  pass on the retry is named in the job's annotations and counts as an
  unrelated failure.
- After a test's second unrelated failure, the hub adds it to
  `.config/quarantine.txt` with its issue, by PR and without asking:
  `<nextest filter>  #<issue>  <expires YYYY-MM-DD>`, or
  `android:<class>#<method>`, or `ios:<target>/<class>/<method>`.
- Every tier skips a quarantined test except `quarantined`, which runs the
  list nightly without failing. A line past its date fails `scripts` until
  the test is fixed or a PR renews the date and says why.
- Tests that measure speed, take screenshots or survey layouts stay off the
  queue; the timing tests run nightly.

## Detection snapshots

- Hosted `windows-latest` is the reference platform, because a developer
  machine's scores differ in the fourth decimal place.
- Never commit your own `--refresh-snapshots` output; it is only a draft
  for a new fixture. Push, let the head's detection gate fail,
  run `just snapshots-fetch <PR>`, review the diff, then commit and push.
- After a runtime or model pin change, record every snapshot with
  `gh workflow run ci-detection.yml --ref <branch> -f record=true`.

## Changing CI

- A change to what decides merges (`merge-gate.sh`, `queue.yml`, the job
  workflows, `queue-runner.sh`, `ci-areas.txt`) first passes a shadow run on
  `ci-shadow/main/<name>`, with a deliberately broken twin that must fail.
  Delete both branches afterwards.
- Name a summary job's parts `<summary> (<part>)`, or the gate counts their
  capacity cancels as failures.
- If a merged gate change blocks the queue, the maintainer pushes its revert
  straight to `main`.

## iOS tests on the Mac

- Run only the test classes you touched
  (`just ios-test "iPhone 17" mdocUITests/<Class>`), one heavy build at a
  time after checking `uptime`: the full suite takes 25 to 30 minutes on the
  fanless Air; parallel runs overheat it.
- On macOS 27, Xcode 26.2's test runner hangs: set
  `DEVELOPER_DIR=/Applications/Xcode-27.2.app/Contents/Developer` (the
  pinned iOS 26.2 runtime still). Hosted `ci/ios` stays the reference.
