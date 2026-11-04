# CI on a merge queue

Status: Accepted, 2026-11-02. Supersedes [ADR 0082](0082-local-ci.md)
(local CI as the per-head gate, the Mac runner, the fallback thresholds,
runners testing `main`). Amends [ADR 0070](0070-ci-speed-and-script-hygiene.md)
(runners), [ADR 0091](0091-mobile-ci-and-releases.md) (where mobile tests
run, the fallback, the takeover, areas) and
[ADR 0093](0093-flaw-hunt-loop.md) (the detection gate's place, reference
platform and snapshots). Rolled out by ROADMAP's CI redesign section; in
force since the cutover (item 6), with the amendments at the end.

## Problem

On 2026-11-02 two hubs merged about 15 PRs in a day and spent much of it on
CI, not on the PRs ([interview](../design/ci-redesign-grill-2026-11-02.md)):

- The Mac runner shares the dev Mac with the hub and builder lanes. Its
  GPUI workspace tests stalled 80–120 s and hit nextest's kill on PRs that
  never touched the app (#510: an unsigned test binary's signature query
  makes `syspolicyd` read a 59 GB `target/debug/deps`). Hosted `macos-26`
  ran the same suite in 288 s; the Mac's `just test` failed after 12–17
  minutes.
- Each machine runs one job at a time, every new head reruns everything,
  and a Mac PR waited 45–60 minutes. A rules-only PR paid 15–19 minutes of
  Android.
- The hosted fallback posted green statuses that tested nothing ("NOT
  tested" mobile) or less (no `just mobile`, no detection gate), so the
  hub read every description.
- Runners tested `refs/pull/N/merge`, which GitHub rebuilds only when the PR
  changes: results named a `main` hours old, and the hub re-tested every
  PR against current `main` by hand.
- Model snapshots reproduced only on the Mac; logs were not readable across
  machines; runner upkeep (reinstalls, output, a 74 GB `target/`) was by
  hand; a new `cargo deny` advisory turned `main` and every PR red.

## Decision

### What a status means

- **A required status is `success` only when its job ran in full.** "Not
  affected" and "Docs only" stay green: they are decided from the diff. A
  job with nowhere to run stays pending; nothing posts "NOT tested" or a
  partial result as green.
- **Goals, in order:** a green gate means tested; CI runs unattended for
  hours; the dev Mac stays free for interactive work; short push-to-merge
  time; little runner upkeep. Free only: hosted runners and the existing
  machines.

### The merge queue

- **GitHub's merge queue on `main`**, by a ruleset that requires `lint
  (ubuntu)` and `merge-gate`, with a bypass for the maintainer's direct
  pushes (`just release`). Agents enqueue with `gh pr merge --auto` and
  never use `--admin`. Groups of 1–4 PRs, merging fewer after 5 minutes;
  build concurrency 2. A failing group drops the failing PR and retests the
  rest.
- **Two stages.** Every PR head runs the **fast tier**: `lint (ubuntu)`
  (fmt, cargo-deny, script lint, Linux clippy) and `unit`, `scripts` and
  `just mobile --ffi` on hosted Ubuntu (hosted macOS for a crate whose
  tests cannot run on Linux). The **queue commit** (`main` plus the group,
  on `gh-readonly-queue/main/*`) runs the full suite of the areas the group
  touches, once, right before the merge. A rebase reruns only the fast
  tier.
- **`merge-gate`** is one aggregate status. On a PR head it is green when
  the fast tier passed and says so; on a queue commit, only when every
  affected full job passed, each named with where it ran. The per-job
  statuses (`ci/macos`, `ci/windows`, `ci/ios`, `ci/android`,
  `ci/detection`) are shown, not required, so moving or adding a job never
  touches the ruleset. A hosted workflow runs `main`'s aggregator script
  and never checks out PR code.

### Where jobs run

| Job | PR head | Queue commit | Fallback |
|---|---|---|---|
| lint | hosted Ubuntu | hosted Ubuntu | — |
| fast tier | hosted Ubuntu | — | — |
| macOS desktop (`unit`, `app`, `renders`) | — | hosted `macos-26` | — |
| iOS job | — | hosted `macos-26` | — |
| detection gate | — | hosted `macos-26`, sharded | — |
| Windows desktop | — | the PC | hosted `windows-latest` |
| Android job | — | the PC, beside Windows | hosted Ubuntu with KVM |

- **The Mac is not a CI runner.** ADR 0091's Mac takeover is dropped.
- **The PC runs two jobs at once** (Windows desktop and Android) for queue
  commits only; it no longer tests PR heads or `main`.
- **Fallback:** a queue commit's PC job runs hosted when the PC's heartbeat
  is older than 10 minutes or the job has not started 10 minutes after the
  queue commit appeared. Whichever result arrives first counts and
  `merge-gate` names where it ran. ADR 0082's 15/45/50-minute thresholds
  and per-head overflow go.
- **Third-party code:** a queue group with a fork or bot (Dependabot) PR
  runs its Windows and Android jobs hosted, never on the PC. PR heads of
  forks and bots run the fast tier only, as everyone's do.
- **Lanes** run what they touched before ready (`just test -p <crate>`, the
  `app` tier for UI changes) and may dispatch the full hosted suite on
  their branch; the queue is the gate. development.md says which tier each
  kind of change runs locally.

### Test tiers

- `just test` keeps running everything locally; CI runs tiers as nextest
  filtersets and test groups:

  | Tier | What | Warn | Kill |
  |---|---|---|---|
  | `unit` | crates and pure logic | 30 s | 2 min |
  | `app` | GPUI workspace tests, a test group of at most 4 threads | 30 s | 2 min |
  | `renders` | `headless_renders` (its watchdog stays) | — | 11 min macOS, 21 min Windows |
  | `recognition` | fixtures on their snapshots | 30 s | 2 min |
  | `scripts` | the script suites, also under macOS `/bin/bash` 3.2 | — | job limit |

- **Job limits:** fast tier 15 minutes, macOS desktop 30, iOS 40, Windows
  30, Android 40, each detection-gate shard 10.
- **Test builds never call OS security or identity services**
  (code-signing queries, Gatekeeper, Keychain, the real profile): a seam
  tests replace, and a guard test that fails when a test binary reaches
  one. Production paths are unchanged. A slow `app` test is a bug, not a
  timeout to raise.
- **Flakes:** infrastructure steps retry once, logged (emulator boot,
  Gradle and network fetches, `setup.sh` downloads); tests never retry. A
  test that flakes twice goes into `.config/quarantine.txt`, one line per
  test: `<nextest filter>  #<issue>  <expires YYYY-MM-DD>`. CI excludes it
  and runs the list alone without failing anything; the fast tier fails on
  an expired line. Entries come by PR; the hub may add one.

### Areas

- ADR 0091's areas stay, plus **recognition code**
  (`crates/mdoc-detect/**`, `crates/mdoc-pii/**`, the rule pack included):
  `unit`, `recognition`, the detection gate, desktop `app` and `renders` on
  macOS and Windows, and `just mobile --ffi`; no iOS or Android app job (the
  desktop parity test and the FFI check cover the bindings). The
  fixtures-only area stays as ADR 0082 amended it.

### Detection gate

- **Hosted `macos-26`**, the FP16 model downloaded from its pinned
  Hugging Face revision and cached; fixtures shard over parallel jobs once a
  run passes 3 minutes. Supersedes ADR 0093's "the Mac is the reference
  platform" and its per-PR/nightly split.
- **Reference platform.** A first experiment checks that a hosted runner
  reproduces the M4's snapshots byte for byte. If it does not, the hosted
  runner becomes the reference and every snapshot is re-recorded there
  once. Never a tolerance.
- **Snapshots.** A failing gate uploads regenerated snapshots as an
  artifact; `just snapshots-fetch <pr>` applies them on any machine for an
  agent to review and commit. No workflow writes to PR branches.

### `main`, supply chain, logs and upkeep

- **`main`** receives tested queue commits (release commits are tested by
  their tag's run). Its pushes run cache-saving builds only (hosted macOS
  and Windows `--no-run`, and lint), since queue-branch caches are not
  shared. A nightly full run on `main` of every hosted job, fallbacks
  included, opens an issue when red.
- **cargo-deny:** licences, bans and sources on every PR; advisories on PRs
  that change `Cargo.lock` and in a daily scheduled run that opens an
  issue. An advisory with no fix yet gets a dated `deny.toml` ignore with
  its reason.
- **Logs:** hosted jobs keep theirs on GitHub. The PC's stay on its tailnet
  page with the Android HTML report and nextest's failing-test list copied
  there, and the failing step and tests named in the status and summary.
  No external log store.
- **PC runner upkeep:** it reinstalls itself from `main`'s copy when idle
  and outdated; deletes `target/` past 60 GB or when the toolchain
  changes; deletes output past 7 days or 20 GB, oldest first,
  with `rm`; refuses jobs below 40 GB free and says so; writes a heartbeat.

## Invariants

- No PR merges without a full result for every affected area on `main` plus
  that PR, from the PC or a hosted runner; never a waiver.
- A green required status describes what ran on that commit.
- Third-party code reaches the maintainer's machines only after review and
  never through the queue (ADR 0091's invariant, kept).
- No workflow that reads PR code can write to the repository.
- ADR 0091's release, signing and changelog decisions are unchanged.

## Consequences

- PR heads give feedback in minutes on hosted runners in parallel; the full
  suite runs once per queue group instead of once per head. Busy days
  queue groups, not heads.
- A break that only shows in the full suite is found in the queue, after a
  lane marked the PR ready; it costs one queue cycle and drops only that
  PR.
- macOS testing depends on GitHub's hosted runners (5 concurrent macOS jobs
  on the Free plan). If hosted macOS cannot run the detection gate, a
  dedicated CI Mac is the fallback plan.
- Windows and Android queue jobs depend on the PC; with it off the queue
  waits 10 minutes, then about 24 minutes of hosted Windows.
- The ruleset is the first branch protection on `main`; ADR 0082's "no
  ruleset" ends. The fleet profile's required checks become `lint (ubuntu)`
  and `merge-gate` (fleet ADR 0017, changed in the fleet repository).
- The dev Mac gives back about 80 GB and the CPU the runner used.

## Rollout

Interim, now and independent: the dated `zip` ignore (`main` is red);
#510's fix, then close #513; delete the Mac CI checkout's `target/`.
Then ROADMAP's CI redesign items: (1) tiers, quarantine, the OS-services
seam; (2) hosted jobs in parallel lanes: macOS desktop and bash 3.2,
iOS, Android on KVM (after ADR 0091 item 3), the detection-gate experiment
and sharding, the fast tier on Linux; (3) the `merge-gate` aggregator; (4)
the PC runner; (5) a shadow trial on 5 PRs beside the current gate,
failing a deliberately broken PR in each tier; (6) cutover: ruleset, queue,
fleet checks, Mac runner retired; (7) `main`'s runs, the nightly and
cargo-deny's split.

## Amendments at the cutover (2026-11-03)

What implementing it settled, by the CI redesign hub:

- **Android has no hosted job yet.** The cutover went ahead with Android on
  the PC only; the hosted Android job (`ci-android.yml`, after Mobile CI/CD
  item 3) is a ROADMAP follow-up. Until then a queue group that needs
  `ci/android` waits for the PC while it is up, and `merge-gate` fails it
  at once, saying why, when it has nowhere to run: third-party code, a PC
  runner error, a stale heartbeat, or a PC that has posted `ci/android` on
  no queue commit and not on `main` for 10 minutes (2026-11-04: with the PC
  offline, #584 otherwise waited for the queue's 2-hour timeout).
- **Detection snapshots are re-recorded on hosted `macos-26`** at the
  cutover (#534): scores only, at most 0.00193 from the M4's, every entity
  the same. The existing 1e-4 check stays. The queue runs 4 shards, and
  CI sets `MDOC_DETECT_THREADS=4`, the count they are recorded with.
- **The PC's heartbeat** is a pending status refreshed at least every 4
  minutes; `target/` goes only past 60 GB or on a toolchain change.
- **Hosted Windows** gets 40 minutes: a cold build takes 32. Each detection
  shard gets 20 until `main` saves its build cache (item 7): uncached, its
  setup compiles for about 6 minutes, and a shard timed out at 10.
- **PR heads run `lint (ubuntu)` on every change**, docs included: a
  required check that never runs blocks a PR. `local-ci-fallback.yml` is
  removed; the local CI scripts go with item 7.
- **Shadow branches** (`ci-shadow/main/*`) run as queue commits without
  merging, for trying changes to CI.

Terms: [glossary](../../CONTEXT.md). References:
[interview](../design/ci-redesign-grill-2026-11-02.md).
