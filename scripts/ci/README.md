# scripts/ci

The merge queue's machinery; lanes need only
[docs/specs/ci.md](../../docs/specs/ci.md). Script and workflow headers say
what each piece does; this page says how they fit and why.

## How the pieces fit

- GitHub's merge queue (repository settings) builds groups of 1 to 4 PRs
  after a 5-minute wait, two groups at a time, each on `main` plus the PRs
  ahead of it.
- One diff decides every job: what a queue commit brings onto its merge
  base with `main`, routed by the commit's own `ci-areas.txt` (except a
  queue commit whose own change is docs only: see
  [ci.md](../../docs/specs/ci.md)). `queue.yml`
  (`ci-areas.sh --queue`), the PC's runners and `merge-gate.sh` all read
  it, so a job one skips, the others do not wait for.
- `merge-gate.sh` alone posts the verdict. `merge-gate.yml` runs `main`'s
  copy, never PR code, so test a change to it with `test-merge-gate.sh` and
  a dry run against shadow commits, not a shadow run's own `merge-gate`.

The hosted jobs are the `ci-*.yml` workflows. The PC posts `ci/windows`
and `ci/android`; `merge-gate.sh` dispatches their hosted fallbacks.

## Hosted iOS

- Keep each Xcode pin in `../tools.env` on what its image ships (tests:
  `macos-26`, builds: `xcode-27`): a runtime the image lacks downloads about
  8 GB per job.
- Parallel Simulator clones (`MDOC_IOS_TEST_WORKERS`) stay off: two clones
  were slower and failed UI tests.
- The organization's policy lets no workflow open a PR, so the hub opens
  and enqueues the nightly's `nightly/ui-class-times` branch with its own
  token.

## Caches

- Only `main` saves caches: a branch restores only its own and `main`'s.
  rust-cache keys by job id, so a job that builds something new needs a
  `main` job of the same id (`ci.yml` or `main-caches.yml`), or every queue
  commit builds it cold.
- A cache that matches its key exactly is never saved again; new builds
  reach it only under a new key (a job id, `Cargo.lock` or the toolchain).

## Rules

- No self-hosted Actions runners and no paid larger runners: on a public
  repository a fork PR could route jobs to a self-hosted machine, so the PC
  polls for queue commits instead. Workflows that read PR code have
  read-only tokens.
- Every script suite runs on every change that could break it, on hosted
  Linux at least.
- Infrastructure steps (emulator boot, Gradle and network fetches, `setup.sh`
  downloads) retry once, logged; tests do not.
- No external log store, and no link checker (it needs the network and
  flakes).
- An advisory with no fix yet gets a dated `deny.toml` ignore with its
  reason.
- A dedicated CI Mac is reconsidered only if capacity re-runs still fail
  more than about once a week.
- The iOS UI shards each build for themselves: a build-once hand-off would
  put a serial build on the critical path to save about 3 runner-minutes.
- Build tools (sccache, other linkers, Cranelift) wait until a job's warm
  build step passes 5 minutes; building takes 2 to 4 minutes of each job.

## Trying it

- Shadow branches (`ci-shadow/main/*`) run as queue commits without
  merging. End their commit subjects "(#N)" so the group is not taken for
  third-party code. A docs-only top commit runs nothing, so a full-suite
  trial puts code and docs in one commit.
- `gh workflow run nightly.yml --repo lawkitt/mdoc` runs the nightly;
  `-f drill=true` runs only the CI targets job, forced red.

## The PC's runners

Two copies of `../queue-runner.sh`, one per job; `queue-runner-setup.ps1`'s
header names their tasks, homes and output folders.

- **Check:** `status.txt` in each output folder has a `heartbeat:` line
  from the last 5 minutes; `queue-runner.sh status` on the PC also checks
  that the tailnet serves `/ci`.
- **Install or update at once:** in PowerShell, not as admin, from a
  checkout of current `main`:
  `powershell -ExecutionPolicy Bypass -File scripts\ci\queue-runner-setup.ps1`.
  An idle runner updates itself from `main`'s `queue-runner.sh` and
  `lib.sh`, which is why both stay at the top of `scripts/`.
- **Stop and start:** `Stop-ScheduledTask 'mdoc queue CI windows'`,
  `Start-ScheduledTask …`; re-run the setup to also end a job's builds.
- **After a restart:** if `status.txt` still says
  `tailnet: /ci not served`, restart the `mdoc queue CI web` task or run
  `tailscale serve --bg --set-path /ci http://127.0.0.1:8787`.
