# Local CI

Status: Accepted, 2026-10-29; amended 2026-10-30, 2026-10-31 and 2026-11-01
(overflow and recognition-only jobs, below). Amended by
[ADR 0091](0091-mobile-ci-and-releases.md): areas decide which jobs run,
`local/ios` and `local/android` join the gate (five checks), and the runners
also test `main`; the hosted mobile jobs and the Mac's takeover follow in its
rollout. Amends [ADR 0070](0070-ci-speed-and-script-hygiene.md) (runners,
test runner, cache) and [ADR 0037](0037-ci-and-release-pipeline.md) (where
CI runs).

## Problem

Up to six agent PRs are open at once. Each push and each merge of `main`
into a PR branch re-runs the hosted matrix, whose test jobs take about
15 minutes on Windows and 10.5 on macOS; the render test alone is 514 s and
406 s of that. Jobs start within seconds: the runners are slow (4 and
3 vCPUs), not scarce. The same render run takes 55 s on the maintainer's
Windows PC. ADR 0070's 8-minute Windows target is missed, and agents and the
hub wait on CI.

## Decision

- **Local CI is the pre-merge test gate.** A runner on the Windows PC and one
  on the Mac test PR heads from branches in `lawkitt/mdoc`, never from forks
  or Dependabot, at the exact commit. Each posts two commit statuses, each
  from its own job: the desktop one, `local/windows` or `local/macos` (`just
  lint` and `just test`; the Mac then the mobile cargo check, `just mobile
  --ffi`, [ADR 0085](0085-mobile-apps.md)), and the mobile one,
  `local/android` (the Android job) or `local/ios` (the iOS job), as
  [ADR 0091](0091-mobile-ci-and-releases.md) describes.
  No self-hosted GitHub Actions runners and no paid larger runners (this
  replaces ADR 0070's runner rule: on a public repository a fork PR can route
  jobs to self-hosted machines, guarded only by the approval click).
- **Hosted CI.** PRs run `lint (ubuntu)` only (fmt, cargo-deny, script lint,
  Linux clippy). Dependabot and fork PRs run the full hosted matrix, so new
  third-party code is never compiled on the maintainer's machines before
  review. `main` runs the full matrix after each merge, saves caches and
  uploads renders (7-day retention); a red `main` stops merges until fixed
  or reverted. The matrix's clippy and test jobs run only when the change
  touches the desktop or core area (ADR 0091). `workflow_dispatch` runs the
  full matrix on any branch. `release.yml` keeps calling the full `ci.yml`
  and never depends on local CI.
- **Merge gate.** A code PR merges with `local/windows`, `local/macos`,
  `local/ios`, `local/android` and `lint (ubuntu)` green. The agent waits for
  them and fixes failures; the hub merges one PR at a time and checks the
  same five. A PR that is
  behind `main` but merges cleanly is not re-tested; no GitHub merge queue
  and no ruleset (`just release` pushes to `main` directly).
- **Hosted fallback** (2026-10-31). When a head's `local/macos` or
  `local/windows` is stuck (no status 15 minutes after the head appeared,
  a capacity wait for 15, a step running for 50, otherwise pending for 45),
  a scheduled workflow (`local-ci-fallback.yml`, every 10 minutes and after
  each CI run) dispatches the full hosted `ci.yml` on the PR branch and
  posts that OS's `clippy` and `test` result under the stuck context,
  prefixed "Hosted fallback". It replaces the manual stand-in (a dispatched
  `test (windows-latest)` for `local/windows`, and nothing for
  `local/macos`, so with the Mac off nothing merged). A stuck `local/ios` or
  `local/android` is posted `success` "Hosted fallback: NOT tested; no hosted
  iOS job yet (ADR 0091 item 5)" at once: temporary, until ADR 0091's hosted
  mobile jobs replace it, so a machine that is off never blocks merges. The
  gate keeps its five checks; a green fallback status counts. A runner
  tests again only a fallback `error` and a mobile "NOT tested"; on the Mac
  a fallback pass of `local/macos` gets the detection gate only (2026-11-01,
  below). The fallback re-reads a status before writing over it. The fallback runs `main`'s
  script with `statuses: write` and `actions: write` and never checks out
  PR code. Accepted gaps: hosted CI tests the branch head, not the merge
  ref, and has no `just mobile` or mobile job yet, which the status says.
- **Runner.** `scripts/local-ci.sh` behind `just local-ci`: polls open
  non-draft PRs every 60 s with the machine's `gh` login, one queue per
  machine holding each PR's desktop and mobile job (oldest status first, a
  PR's desktop job before its mobile one), a newer head of a PR replaces its
  older jobs, statuses `pending` then `success` or `failure`. It tests
  `refs/pull/N/merge` as it stands when the run starts and posts on the PR
  head, naming the `main` commit; a PR with conflicts gets `failure`.
  Without a build, as soon as the merge ref is there, a status whose area
  the PR does not touch (`scripts/ci-areas.txt`, ADR 0091) gets `success`
  "Not affected", and a PR that touches only files in no area (the table's
  `none` lines, which `ci.yml`'s `paths-ignore` repeats word for word, a
  test checks) gets `success` "Docs only" on both. With no PR job queued, it
  tests each new `main` commit in the areas that commit touched and posts
  its statuses there (ADR 0091). A launchd agent on the Mac; a Task Scheduler
  task at logon on Windows, with its own clone at `D:\ci\mdoc`. The Mac
  runner is a build slot beside the two agent builders and starts a run
  only while memory pressure is normal and 40 GB is free (2026-10-31: was
  swap under 2 GB, but macOS keeps swap after memory frees, and that limit
  held every job for four hours with nothing on GitHub to say why). Queued
  heads say what the runner waits for, and each step posts its own
  `pending` status. Machine settings live in the runner home's
  `runner.env`, which install keeps. Job limits match the hosted ones.
- **Logs and renders** stay on the machine that ran them for 7 days, served
  to the tailnet only (`tailscale serve`); each status links there. The hub
  views renders there instead of downloading the hosted artifacts.
- **Timeouts.** Hosted jobs: `timeout-minutes` 15 (lint), 20 (clippy), 30
  (test).
- **cargo-nextest** replaces `cargo test` for the workspace tests (replaces
  ADR 0070's "for now"): slow-test warning at 60 s, kill at 2 min, the render
  test overridden to its 10-minute (macOS) and 20-minute (Windows) watchdog;
  no retries, no JUnit; doctests through `cargo test --doc`; version pinned
  in `scripts/tools.env`.
- **Cache generations.** After `main` saves a cargo cache, a `main`-only
  step deletes older generations of the same job's key; it is the only
  `actions: write` permission in `ci.yml`.
- **Render files.** Headless scenes live in one module per feature; the
  render test's screen list is a one-name-per-line file with `merge=union`.
- **Mac render speed.** ADR 0070's per-crate `opt-level` 2 rule applies to
  the macOS hot path (`gpui-pre-apple`, `image`, `fdeflate`), macOS gets the
  background PNG encoder thread, and the render test is sharded into
  separate nextest processes only if the M4 run misses its target. The
  render test stays on every PR with all of its evidence.
- **Windows run time** (#240). Each Windows job had built cold, its
  `git clean -e /target/` turned into a Windows path by Git Bash; the job
  now keeps `target/`. The Task Scheduler task runs at normal priority, not
  the default below normal. `gpui-pre` and `taffy` get ADR 0070's
  `opt-level` 2 (render run 215 s to 105 s alone; cold Windows test build
  150 s to 164 s, +9%; cold macOS test build 84 s to 92 s, +10%, measured
  on the M4 on 2026-10-30). `just test` runs the script suites beside the Rust
  tests. The render test stays one process with all of its evidence.
- **Targets** (push to both local statuses green, empty queue, warm CI
  checkout): `local/windows` and `local/macos` 4 minutes each; 8 minutes
  when `Cargo.lock` or the toolchain changes. Hosted `main` and release runs
  have no target beyond their job timeouts. This replaces ADR 0070's
  8-minute hosted Windows target.
- **Rollout.** Docs first; then hosted hygiene (timeouts, nextest, cache
  cleanup, retention), render speed and render files, and the runner, in
  parallel; then the cutover of PR CI to `lint (ubuntu)`, only after local
  statuses have agreed with hosted CI on five PRs and failed a deliberately
  broken test branch. Trial result: local and hosted agreed on six PRs
  (#237, #238, #241, #244, #245, #246), and both runners failed the
  deliberately failing #239; PR CI cut over on 2026-10-30 (#252).

- **Overflow and recognition-only jobs** (2026-11-01). On 2026-11-01 the
  Mac was asked to test 122 PR heads and finished 56; up to 15 PRs were
  open, and its desktop job took 23 minutes instead of 2 (the same GUI test
  6 s in the morning, 100 s in the afternoon) while it also ran about 16
  agent sessions. One job at a time per machine is a queue, not a speed-up,
  at that volume. So: (1) area `recognition` (`tests/fixtures/recognition/`,
  ADR 0093): a PR touching only it runs `setup`, `cargo nextest run -p
  mdoc-detect` and the detection gate in `local/macos`, and `local/windows`
  is "Not affected" (6 of the 15 open PRs were such); (2) the fallback
  claims a "Queued" desktop head that has two or more heads ahead of it for
  5 minutes (overflow), since hosted CI is free on this public repository
  and runs side by side; (3) runners do not test again what hosted CI took:
  heads it is running or failed, and off the Mac those it passed.

## Consequences

- A PR's tests finish in minutes instead of 10–15, and a merge of `main`
  into a PR costs one local run, not a hosted matrix.
- Pre-merge Windows and macOS testing is fast while the maintainer's PC and
  Mac are on. When either is off or stuck, the hosted fallback gives a
  result in about 30–45 minutes (the stuck time, a scheduled run that may
  start late, then about 15 minutes of hosted CI), without the hub.
- Local results are not in GitHub's checks UI beyond the statuses, and their
  logs and renders are reachable only inside the tailnet and only for 7 days.
  Hosted `main` runs still keep 7 days of renders on GitHub.
- The PC and the Mac each keep one more build directory (~30 GB; on the PC
  on `D:`). The Mac can build three things at once: two agents and CI.
- A clean merge that breaks only in combination is caught on `main` after
  the merge, not before; a red `main` stops merges until fixed.
- Dependabot PRs still take the hosted ~15 minutes; nobody waits on them.
- Local runs execute PR code under the maintainer's account with their `gh`
  login, as the lanes already do; only branches pushed to `lawkitt/mdoc` by
  someone with write access are tested.
- The fleet skill reads this through fleet ADR 0017: required checks
  (profile `checks`: the five gate checks), renders by the status links (set
  with the runner), ready-after-gate lanes, a watched `main`, and the hub's
  roadmap commits.

References: [interview](../design/ci-speed-3.md).
