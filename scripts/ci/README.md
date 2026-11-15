# scripts/ci

The merge queue's machinery. Lanes need only
[docs/specs/ci.md](../../docs/specs/ci.md); each script's header documents
what it does in detail.

## The queue

- GitHub's merge queue on `main` builds groups of 1 to 4 PRs after a
  5-minute wait, two groups at a time, each on `main` plus the PRs ahead of
  it, on a `gh-readonly-queue/main/*` branch. A failing group drops its
  failing PR and retests the rest.
- `queue.yml` runs `ci-areas.sh --queue`, which diffs the queue commit
  against its merge base with `main` and starts the hosted jobs its areas
  need. The PC's runners and `merge-gate.sh` read the same diff with the
  commit's own `ci-areas.txt` (`main`'s when GitHub cannot give it).
- `merge-gate.yml` (job `merge-gate pass`) runs `main`'s `merge-gate.sh`,
  never PR code, after each run and status, from `merge-gate-recheck.yml`,
  and every 5 minutes (GitHub runs that schedule rarely; nothing relies on
  it). The header covers the head and queue rules, `gh_call`'s retries,
  re-runs of jobs that never got a runner, and the hosted fallback.
- A change to `merge-gate.sh` shows in `test-merge-gate.sh` and a dry run
  against the shadow commits, not in a shadow run's own `merge-gate`
  status, since the workflow runs `main`'s script.

## Jobs on a queue commit

| Status | Areas | Runs on | Limit |
|---|---|---|---|
| `ci/macos` | desktop | hosted `macos-26` (`ci-desktop.yml`): `(checks)` lint, the suites under `/bin/bash` 3.2 and `just mobile --ffi`; `(tests)` the unit, recognition, app and renders tiers | 20, 30 min |
| `ci/windows` | desktop | the PC (`../queue-runner.sh`): `just lint`, `just test-tier rust` | 30 min |
| `ci/ios` | ios | hosted `macos-26` (`ci-ios.yml`): `(unit)` the unit and parity tests, then `just ios-release`; `(ui 1)`, `(ui 2)` the UI classes split by `../ios/ios-ui-shards.sh` | 55 min each |
| `ci/android` | android | the PC: `just android` | 40 min |
| `ci/detection` | recognition | hosted `windows-latest` (`ci-detection.yml`), 12 shards, each warning past 80% of its 5-minute model budget | 20 min a shard |
| `ci/scripts` | ci without desktop | hosted (`ci-scripts.yml`): the suites on `ubuntu-latest` and under bash 3.2 on `macos-26` | 10 min |

- A PC job runs hosted (`ci-desktop.yml -f os=windows`, or
  `ci-android.yml`, both with `-f ref=<sha>`) when the PC posted nothing
  10 minutes after the queue commit appeared, its heartbeat is 10 minutes
  old, it posts `error`, or the group holds a fork or bot PR. The result
  that finishes first counts.
- iOS jobs pin Xcode 26.2 and the iOS 26.2 runtime build 23C54
  (`XCODE_VERSION`, `IOS_SIM_RUNTIME_BUILD` in `../tools.env`), which the
  `macos-26` image ships. A pin the image lacks downloads about 8 GB per
  job, and a version alone is ambiguous (`26.2` resolves to the 23C52 RC).
  `../ios/ios-toolchain.sh` installs the runtime on a Mac; `just doctor`
  checks both pins.
- Parallel Simulator clones (`MDOC_IOS_TEST_WORKERS`) stay off: two clones
  were slower and failed UI tests (#617).
- The nightly's `UI class times` job refreshes `apps/ios/ui-class-times.tsv`
  (`../ios/ios-ui-class-times.sh`) on the `nightly/ui-class-times` branch;
  the organization's policy lets no workflow open a PR, so the hub opens
  and enqueues it with its own token.

## `main` and caches

- A push to `main` runs no tests. `ci.yml` runs lint and `just mobile --ffi`
  and builds the tests without running them; `main-caches.yml` builds the
  iOS core and sets up the detection model. Together they save every cache
  the queue's hosted jobs restore.
- Only `main` saves caches, since a branch restores only its own and
  `main`'s. rust-cache keys by job id, so a hosted job restores what `main`'s
  job of the same id saved; a job that builds something new needs a `main`
  job of its id, or every queue commit builds it cold.
- A cache that matches its key exactly is never saved again; new builds
  reach it only under a new key (a job id, `Cargo.lock` or the toolchain).
- Neither workflow cancels a run on `main`: a run cancelled while saving
  leaves its key reserved and unwritten until the key changes.
- `../ios/ios-core-cache.sh` keys the built iOS core; `ci-cache-cleanup.sh`
  deletes older cache generations after each `main` run.

## Nightly

`nightly.yml` runs every hosted job on `main` at 02:17 UTC, the timing
tests, the quarantined tests and cargo-deny's advisories.
`nightly-issue.sh` keeps one "Nightly CI is red" issue open while it is red.
Its `CI targets` job runs `ci-stats.sh --days 7 --nights 3` through
`ci-targets-issue.sh`, which keeps a "CI targets missed" issue open while a
target misses on each of 3 nights; a missed target alone does not turn the
night red. Run it with `gh workflow run nightly.yml --repo lawkitt/mdoc`
(`-f drill=true` runs only the targets job, forced red).

## Rules

- No self-hosted Actions runners and no paid larger runners: on a public
  repository a fork PR could route jobs to a self-hosted machine, so the PC
  polls for queue commits instead.
- Every script suite runs on every change that could break it, on hosted
  Linux at least.
- Infrastructure steps (emulator boot, Gradle and network fetches, `setup.sh`
  downloads) retry once, logged; tests do not. Linux package installs get
  8 minutes and two retries, the second on kernel.org's HTTPS mirror.
- Logs stay on GitHub and the PC's tailnet page; no external log store.
- A dedicated CI Mac is reconsidered only if capacity re-runs still fail
  more than about once a week.
- The iOS UI shards each build for themselves: a build-once hand-off would
  put a serial build on the critical path to save about 3 runner-minutes.
- Build tools (sccache, other linkers, Cranelift) are reconsidered only when
  a job's warm build step passes 5 minutes; building takes 2 to 4 minutes
  of each job.

## Supply chain

- `lint (ubuntu)` checks cargo-deny's licences, bans and sources on every
  change, and advisories only when `Cargo.lock` or `deny.toml` changes
  (`deny-ci.sh`). An advisory with no fix yet gets a dated `deny.toml`
  ignore with its reason.
- Fork and bot PRs run the fast tier and the full hosted matrix on the PR;
  their queue groups run hosted only, never on the PC. Workflows that read
  PR code have read-only tokens.
- Shadow branches (`ci-shadow/main/*`) run as queue commits without merging.
  Their commit subjects end "(#N)" so the group is not taken for
  third-party code.

## The PC's runners

Two copies of `../queue-runner.sh` (its header has the statuses, limits,
upkeep and output) run on the Windows PC, one per job, homes on the Dev
Drive:

| | Windows job | Android job |
|---|---|---|
| Home | `E:\ci\queue-windows` | `E:\ci\queue-android` |
| Task (at logon, no window) | `mdoc queue CI windows` | `mdoc queue CI android` |
| Results | `E:\ci\out\queue-windows\<sha>\` | `E:\ci\out\queue-android\<sha>\` |
| Tailnet | `https://windows.<tailnet>.ts.net/ci/queue-windows/` | `…/ci/queue-android/` |

- **Check:** `status.txt` in each home's output folder has a `heartbeat:`
  line from the last 5 minutes; `queue-runner.sh status` on the PC also
  checks that the tailnet serves `/ci`.
- **Install:** in PowerShell, not as admin, from a checkout of current
  `main`: `powershell -ExecutionPolicy Bypass -File scripts\ci\queue-runner-setup.ps1`
  (`-Uninstall` removes the tasks and keeps the homes). The first Windows
  build fills about 30 GB.
- **Update:** an idle runner copies `main`'s `queue-runner.sh` and `lib.sh`
  when they differ and restarts, which is why both stay at the top of
  `scripts/`. Re-run the setup to update at once.
- **Stop and start:** `Stop-ScheduledTask 'mdoc queue CI windows'`,
  `Start-ScheduledTask …`; re-run the setup to also end a job's builds.
- **After a restart:** the `mdoc queue CI web` task serves `/ci` again,
  retrying for 5 minutes. If `status.txt` still says
  `tailnet: /ci not served`, restart that task or run
  `tailscale serve --bg --set-path /ci http://127.0.0.1:8787`.
