# CI fixes grill, 2026-11-06

Interview with the maintainer on the CI faults and slow spots found after
the merge-queue cutover ([ADR 0095](../adr/0095-ci-on-a-merge-queue.md)).
Decisions are recorded per round; the ADR follows when the tree closes.

## Facts found before round 1

From the CI investigation of 2026-11-05 (runs 2026-11-03 to 2026-11-05) and
the Windows PC's findings in #614:

- **Stuck gate (#613).** A `merge-gate` pass hit a GitHub API error,
  skipped the PR silently (`|| continue` in `heads_pass`) and ended green.
  The `*/5` schedule ran 10 times in 3 days, so nothing retried. 15 of 400
  recent passes hit HTTP 500/502 or truncated JSON; 13 still ended green.
- **Missed enqueue (#613).** GitHub normally enqueues 18–54 s after the gate
  turns green with auto-merge on; #613 waited 6 minutes until auto-merge was
  toggled. Its green came from the only manual re-run of a
  `pull_request_target` gate run in 3 days. The cause can't be seen through
  the API.
- **Hosted macOS capacity.** Jobs not acquired by a runner end `cancelled`,
  and `merge-gate` counts that as failure: 6 of 20 `failed_checks` dequeues,
  plus #613 on 11-05. Three PRs then sat out of the queue overnight.
- **Dequeue causes (20).** 8 infrastructure, 5 flaky tests (emulator focus
  ×3, `OriginalTest`, a timing check in `test-queue-runner.sh`), 7 real
  (5 stale detection snapshots, #608, #587).
- **`main` red on hosted Windows since 2026-11-03 14:48Z.** The render test
  hits nextest's 20-minute kill (926–970 s when it passes; 280–332 s on the
  PC), and the disk-floor block of `test-queue-runner.sh` times out on a
  slow host (its fake runner settles about 15 commits first; the test waits
  64 s). ADR 0095 says `main` runs cache-saving builds only, but `ci.yml`
  still runs full tests there.
- **No iOS Release build in CI.** Every job builds Debug for the Simulator;
  #613 fixed a Release-only compile error found at archive time.
- **PC.** The tailnet `/ci` page was gone (no serve config) for about a day
  until restored on 11-05. Windows output keeps ~1.1 GB of renders per run,
  so the 20 GB cap holds about a day. Gradle prints no test names for
  emulator failures; the JUnit XML has them. Every PC flake ran under load
  from the other PC job.
- **Speed.** One queue commit takes a median of 21.8 min to a green gate
  (p90 30). The critical path is `ci/macos` (20 min: `mobile --ffi`
  recompiles 127 crates, ~4.6 min) and `ci/ios` (18 min: the iOS 26.1
  Simulator runtime, ~8 GB, downloads every run, ~3.5 min; the core builds
  in the optimized `mobile` profile, ~4.7 min). Stacked PRs go through the
  queue one at a time: 7 PRs took 5 hours on 11-05.
- **Budget.** The repository is public on the Free plan: hosted minutes are
  free; 5 concurrent macOS jobs. Detection shards wait p90 26 min for a
  runner; lanes' manual detection runs wait a median of 33 min.

## Decisions

### Round 1 (all recommendations accepted)

The maintainer's steer: CI slowness has cut iteration speed a lot, so speed
comes first.

1. **Record.** A new ADR 0100, "CI reliability after the cutover", amends
   ADR 0095. The work goes in as new numbered items in ROADMAP's "CI
   redesign" section.
2. **Gate recovery.**
   - Every `gh` call in `merge-gate.sh` goes through one wrapper: bounded,
     retried with backoff, and naming the endpoint when it fails.
   - A PR or queue commit skipped because of an error is logged, and the
     run is red.
   - A pass that leaves anything pending, or hit an error, dispatches one
     follow-up pass a few minutes later (`workflow_dispatch`, allowed for
     `GITHUB_TOKEN`).
   - The `*/5` schedule stays, but nothing relies on it.
3. **A hosted job that never ran.**
   - `merge-gate` tells "never ran" (cancelled with no runner, or a
     detection summary over shards that never ran) apart from a result.
   - It re-runs such a job up to 2 times (`gh run rerun --failed`) and stays
     pending meanwhile.
   - After that it fails, worded as infrastructure ("no macOS runner").
   - A real failure still fails at once.
4. **Missed enqueue.**
   - The gate warns (a log line and an annotation) when an open PR has
     auto-merge on, both required checks green, and no queue entry after
     5 minutes.
   - The docs say to re-check the gate with `gh workflow run merge-gate.yml`,
     never by re-running the PR's `pull_request_target` run.
   - No auto-repair, and no new permissions.
5. **`main`.**
   - Main runs lint plus `--no-run` builds on macOS and Windows, as ADR 0095
     says, including the iOS and Android targets that `just mobile --ffi`
     checks, so their deps are cached.
   - The nightly stays the full hosted check that opens an issue.
   - The disk-floor test is fixed anyway: the hosted Windows fallback and
     the nightly run it.
6. **iOS Release.** `ci/ios` also builds the Release configuration for the
   Simulator, unsigned and without tests, on every queue commit that needs
   `ci/ios`.
7. **PC logs.**
   - They stay on the tailnet; ADR 0095's "No external log store" stands.
   - `/ci` is made reliable (8.1).
   - Statuses name the failing tests, Android included.
   - `summary.txt` carries a short excerpt of the failing step.
8. **PC upkeep.**
   1. The web task re-applies `tailscale serve --bg --set-path /ci …` at
      start. `queue-runner.sh status` and the fleet preflight check the
      tailnet URL, not only the local file.
   2. Windows runs keep their renders only when they fail.
   3. The runner copies Gradle's JUnit XML and names failing Android tests
      in the status (#614's parser).
9. **Hosted Android job.** It is built right after the gate fixes, as the
   PC's fallback (PC first, as ADR 0095 planned). Whether hosted becomes
   primary is decided with the drill's timings.
10. **`ci/ios` time.**
    - It uses the image's Simulator runtime when its major.minor matches
      the pin, and downloads only on a mismatch.
    - The core for the tests stays in the `mobile` profile.
11. **macOS budget.** Fewer detection shards (from 4); split `ci/macos`
    later. *Revisited in round 2:* 2 shards would break the gate's
    5-minute model budget.
12. **Areas.**
    - A new `ci` area covers `scripts/merge-gate.sh`,
      `scripts/queue-runner.sh`, the workflows and the CI test suites. It
      runs lint, the scripts tier and the bash 3.2 suites.
    - Changes to the queue machinery also get a shadow-branch run.
    - Recognition code keeps its desktop jobs.
13. **Stacks.** Lanes make independent PRs off `main` where they can,
    enqueued together so they batch. Stacks are kept for changes that truly
    depend on each other. This rule goes in development.md and the fleet
    profile.
14. **Small fixes.**
    - Detection messages print scores to enough digits.
    - "Install Linux build deps" gets a 5-minute limit and one retry.
    - The gate's job is renamed.
    - The gate reads the queue commit's own area table.
    - The disk-floor test gets its own prefix, and the double count is
      fixed.
    - `stale_pass` stops logging "cannot cancel" noise.

### Round 2: speed (all recommendations accepted)

The aim is a queue commit green in about 11 minutes, down from a median of
21.8.

15. **Detection shards: 3** (corrects 11). `scripts/detection-gate.sh` fails
    a model run over 5 minutes, and one shard runs 50 fixtures in 188 s, so
    2 shards (~105 fixtures, ~395 s) would fail every time. 3 shards run
    ~70 fixtures in ~265 s. The gate warns when a shard's model run passes
    80% of its budget: the signal to go back to 4.
16. **`ci/macos` splits into two jobs.**
    - Job A: `just lint`, `just mobile --ffi` and the bash 3.2 suites.
    - Job B: the unit, recognition, app and renders tiers.
    - A summary job named `ci/macos` (as `ci/detection` has) keeps the
      gate and the docs unchanged.
    - Target about 11 min. The 6th macOS job of a group waits about a
      minute, off the critical path.
17. **`ci/ios`.**
    - Tests run on 2 Simulator clones as a measured trial, kept only if
      faster and stable over a day of queue runs.
    - The Release build moves to its own parallel job, `ci/ios-release`.
    - The core for tests stays in the `mobile` profile.
    - Target about 10–12 min.
18. **Runner budget.** The queue keeps building 2 groups at once. A
    dedicated CI Mac is reconsidered only if capacity re-runs (3) still fail
    more than about once a week.
19. **Fast tier.** Two parallel Linux jobs, `fast (unit)` and
    `fast (scripts)`; the head gate waits for both. Target 4 min or less.
20. **Detection on PR heads.**
    - Recognition-area PR heads run the detection gate (3 shards), and the
      head `merge-gate` waits for it.
    - Lanes stop dispatching the detection gate by hand.
21. **Android ABIs.** CI builds the core for x86_64 (the emulator) and the
    host only. aarch64 stays compile-checked by `just mobile --ffi` and is
    built in the Android release workflow.
22. **PC load.** No lock between the two PC jobs. A recurring flake goes into
    quarantine.
23. **Targets and measurement.**
    - ADR 0100 sets the targets: queue commit to a green gate, median 13 min
      or less; the fast tier, median 4 min or less; at most 1
      infrastructure-caused queue drop a week.
    - `scripts/ci-stats.sh` (this investigation's queries) measures them,
      by hand at first.
    - It moves into the nightly later, opening an issue after 3 red days.

### Round 3: execution (all recommendations accepted)

The maintainer is away for 5–6 hours. The hub (this session) does all of
the work, and the Windows lane is started by hand at the beginning.

24. **Follow-up pass.** A new `merge-gate-recheck.yml` runs in its own
    concurrency group, where a newer recheck replaces an older one. It
    sleeps 3 minutes, then dispatches `merge-gate.yml`. A pass that left
    something pending, or hit an error, dispatches it.
25. **Hosted Android.** One lane in wave 2 does Mobile CI/CD item 3 (the
    Android scripts on Linux with KVM, and on the Mac), then CI redesign
    item 2 (`ci-android.yml` as the PC's fallback): two PRs, the second
    after the first merges.
26. **Waves.**
    - Wave 0: ADR 0100, the ROADMAP items, the development.md rules and the
      glossary terms, in one docs PR; the fleet profile's stack rule.
    - Wave 1, in parallel and file-disjoint:
      - A, the gate: `merge-gate.sh`, `merge-gate.yml`, the recheck
        workflow, `test-merge-gate.sh`.
      - B, the desktop jobs: `ci.yml`, `ci-desktop.yml`. Summary jobs keep
        the names `ci/macos` and `fast (ubuntu)`.
      - C, iOS: `ci-ios.yml`, `ios-toolchain.sh`, `queue.yml`. `ci/ios`
        becomes the summary of the tests and `ci/ios-release`.
      - D, detection: `ci-detection.yml`, `detection-gate.sh`, the shard
        count.
      - E, the PC (on `windows`): `queue-runner.sh`,
        `queue-runner-setup.ps1`, `test-queue-runner.sh`, the Android core
        build.
    - Wave 2: F, the `ci` area (after A); G, detection on PR heads (after A,
      B and D); H, hosted Android.
    - Wave 3: `scripts/ci-stats.sh`, a measurement, and the
      parallel-testing verdict.
    - One PR per lane, off `main`, never stacked; Codex reviews before each
      PR.
27. **Hub.** This session dispatches, reviews, enqueues and reports.
28. **Safety.**
    - Every PR that changes what decides merges first passes a
      shadow-branch run that includes a deliberately broken case it must
      fail.
    - Lane A's new paths are tested with fakes.
    - Break-glass: if a merged gate change blocks the queue, the
      maintainer reverts it with a direct push to `main` through the
      ruleset's existing bypass. ADR 0100 names this second use.

The tree is closed: the maintainer confirmed the plan on 2026-11-06.

### Rollout notes (hub, 2026-11-05)

- Decision 15 (3 shards) did not hold: on 3 shards each model run took 84%
  of its budget, and the 80% warning fired, so the gate keeps 4 (#616).
- Decision 17's target (`ci/ios` about 10–12 min) is unmet: the image has
  no iOS 26.1 runtime, and parallel testing was slower and flaky. `ci/ios`
  is about 18 min. Pinning the image's runtime, or caching the iOS core, is
  for the maintainer to decide.
- Wave 0 to wave 1 merged in order: #615, #620, #619, #618, #621 (a fix for
  #619's 404 regression), #617, #616.
