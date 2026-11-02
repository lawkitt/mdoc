# CI redesign (interview, 2026-11-02)

Grill of mdoc's whole CI after 0.4.0 recognition run 3 (#471), where two
parallel hubs merging ~15 PRs in a day hit the problems below. Starting
point: the Mac hub's handoff (`.claude/ci-redesign-handoff.md` in the main
checkout, not committed), [ADR 0070](../adr/0070-ci-speed-and-script-hygiene.md),
[ADR 0082](../adr/0082-local-ci.md), [ADR 0091](../adr/0091-mobile-ci-and-releases.md),
[ADR 0093](../adr/0093-flaw-hunt-loop.md).

## Facts looked up (2026-11-02, main at `7d89029`)

- **Hosted macOS is now as fast as the loaded Mac.** Main's push run
  41424470952: `test (macos-26)` 8m40s job, nextest 288 s for 1375 tests,
  the render test 193 s. The Mac's `local/macos` for #511 and #512 the same
  morning: `just test` failed after 12m31s and 17m07s (UI timeouts, #510),
  `just lint` took 11m01s once.
- **Hosted Windows is slow.** Same run: `test (windows-latest)` 23m50s. The
  PC's `local/windows` takes 6–8 minutes.
- **Hosted capacity.** `lawkitt` is an organization on GitHub Free and
  `mdoc` is public: standard hosted runners cost nothing; the Free plan
  allows 20 concurrent jobs, 5 of them macOS. GitHub's merge queue is
  available to public repositories owned by organizations. `main` has no
  branch protection and no ruleset (ADR 0082); `just release` pushes the
  release commit and tag to `main` directly (`scripts/release.sh:140`).
- **Main is red now.** Main's `lint (ubuntu)` failed at 09:56Z in `just
  deny` (the `zip` 8.6.0 advisory via `docxide-pdf`, handoff issue 10).
- **The CI checkout's `target/` is 74 GB** (59 GB in `target/debug/deps`)
  on the Mac, not 30 GB; 62 GB free on the disk. `~/ci/out` is 4.7 GB.
- **The FP16 model is public.** The detection gate's model
  (`gliner2-pii-fp16-e5948986`, 602 MB) comes from Hugging Face
  (`jugaadsrl/gliner2-privacy-filter-PII-multi-onnx`) by the app's
  manifest; "never downloaded" in `detection-gate.sh` means CI reads the
  copy in the app data. A hosted macOS job could download and cache it.
  Not known yet: whether a hosted `macos-26` (M1-class, 3 vCPU) reproduces
  the M4's snapshots exactly.
- **Machines** (fleet skill): Mac M4 10 cores 24 GB (also the hub and
  builder lanes); PC Ryzen 9 3900X 24 threads 32 GB (Windows, and WSL on the
  same box); `ubuntu` i3-6006U 4 threads 7 GB, always on.

## Rounds

### Round 1 (asked 2026-11-02)

All eight recommendations accepted.

1. **Goals, in order:** a green gate means tested; runs unattended for
   hours; the dev Mac stays free for interactive work; short push-to-merge
   time; little runner upkeep. Reliability before speed: the last two
   days' cost was wrong or stuck results, not slow ones.
2. **Budget:** free only, hosted runners and the existing machines. A
   dedicated CI Mac (a Mac mini) only if hosted macOS cannot run the
   detection gate. A second macOS user is rejected: it isolates files, not
   CPU.
3. **A required status is `success` only when its job ran in full.** No
   "NOT tested" and no partial hosted-fallback greens. "Not affected" and
   "Docs only" stay: they are decided from the diff. A job with nowhere to
   run stays pending. Ends ADR 0091's temporary exception.
4. **GitHub merge queue.** Each PR head runs a fast tier; the full suite
   runs once on `main` + PR (or a batch) before the merge. Needs a ruleset
   on `main` with a bypass for `just release`, and runner support for
   `gh-readonly-queue/main/*` commits. Closes ADR 0082's accepted gap (a
   clean merge that breaks only in combination).
5. **Test tiers:** `unit`, `app` (GPUI workspace tests, a nextest test group
   with a thread cap), `renders`, `recognition` (snapshot fixtures; the
   model gate separately), `scripts`; each with its own timeout and place.
   `just test` still runs them all.
6. **Test builds never call OS security or identity services**
   (code-signing queries, Gatekeeper, Keychain, the real profile): a seam
   tests replace, a rule in development.md and a guard test. Production
   paths unchanged (#510).
7. **Supply chain:** licences, bans and sources on every PR; advisories on
   PRs that change `Cargo.lock` and in a daily scheduled run that opens an
   issue; today's `zip` advisory gets a dated `deny.toml` ignore with its
   reason until `docxide-pdf` releases.
8. **Flakes:** infrastructure retries only (emulator boot, Gradle and
   network fetches, `setup.sh` downloads), once and logged; never test
   retries. A test that flakes twice is quarantined into a listed filter
   with an issue and a 7-day deadline; an expired entry fails a check.

Facts after round 1: GitHub's merge queue admits a PR only after the
required checks pass on its head, then waits for the same checks on the
queue's commit (`gh-readonly-queue/main/...` branches for third-party CI);
merge limits set the group size, build concurrency 1–100. The rule pack
(`crates/mdoc-detect/rules/*.toml`) and recognition code are area `core`,
so a rules-only PR runs every job, Android included. Mobile parity goldens
are written by `crates/mdoc-ffi/tests/parity.rs` on desktop; the app tests
check the bindings return those bytes.

### Round 2 (asked 2026-11-02)

All eight recommendations accepted.

9. **Where jobs run.** Per PR head: `lint (ubuntu)` and the fast tier
   (`unit`, `scripts`, `just mobile --ffi`) on hosted Ubuntu (hosted macOS
   for any crate whose tests cannot run on Linux; checked in rollout). In
   the queue: macOS desktop (`unit`, `app`, `renders`) and iOS on hosted
   `macos-26`; Windows desktop and Android on the PC, in two slots side by
   side; the detection gate per 13. Fallbacks: Windows to hosted
   `windows-latest`, Android to hosted Ubuntu with KVM. **The Mac is no
   longer a CI runner** (frees ~80 GB, ends load clashes with lanes); ADR
   0091's Mac takeover of Android is dropped.
10. **Required checks: `lint (ubuntu)` and one aggregate, `merge-gate`.** On
    a PR head it is green when the fast tier passed and says so; on a queue
    commit, only when every affected full job passed, each named with where
    it ran. Per-job contexts (`ci/macos`, `ci/windows`, `ci/ios`,
    `ci/android`, `ci/detection`) are visible, not required. Moving or
    adding a job never touches the ruleset.
11. **Queue settings:** groups of 1–4, merging fewer after 5 minutes; build
    concurrency 2 (the PC's two slots); a failing group drops the failing
    PR and retests the rest (GitHub's behaviour). Agents enqueue with `gh
    pr merge --auto` and never use `--admin`; the ruleset's bypass covers
    the maintainer's direct pushes (`just release`).
12. **A recognition-code area** for `crates/mdoc-detect/**` and
    `crates/mdoc-pii/**`: in the queue `unit`, `recognition`, the detection
    gate, desktop `app` and `renders` on macOS and Windows, and `just mobile
    --ffi`; no iOS or Android app job (the desktop parity test and the FFI
    check cover the bindings). The fixtures-only area stays.
13. **Detection gate on hosted `macos-26`**, the FP16 model downloaded and
    cached, fixtures sharded over parallel jobs past 3 minutes per run. A
    first experiment checks that a hosted runner reproduces the M4's
    snapshots byte for byte; if not, the hosted runner becomes the
    reference and every snapshot is re-recorded there once. Never a
    tolerance. A failing gate uploads regenerated snapshots as an artifact;
    `just snapshots-fetch <pr>` applies them on any machine for an agent to
    review and commit. No workflow writes to PR branches.
14. **Logs:** hosted jobs on GitHub; the PC's on its tailnet page, with the
    Android HTML report and nextest's failing-test list copied there and the
    failing step and tests named in the status and summary. The PC deletes
    output past 7 days or 20 GB, oldest first, with `rm`. No R2.
15. **PC runner upkeep:** reinstalls itself from `main`'s copy when idle and
    outdated; deletes `target/` past 60 GB or when the toolchain or major
    dependencies change; refuses jobs below 40 GB free and says so; writes a
    heartbeat, and a heartbeat older than 10 minutes counts as off for the
    fallback. Script suites also run under macOS bash 3.2 on hosted macOS
    (#468).
16. **`main`:** commits arrive tested from the queue (release commits are
    tested by their tag's run). `main` pushes run cache-saving builds only
    (hosted macOS and Windows `--no-run`, lint); queue-branch caches are not
    shared. A nightly full run on `main` of every hosted job, fallbacks
    included, opens an issue when red. Local runners stop testing `main`.

Facts after round 2: ADR 0094 is taken by #514 (web address category), so
this redesign is ADR 0095.

### Round 3 (asked 2026-11-02)

All seven recommendations accepted.

17. **Third-party PRs:** a queue group with a fork or bot PR runs its
    Windows and Android jobs hosted, never on the PC (ADR 0091's invariant
    kept word for word).
18. **Lanes** run what they touched before ready and may dispatch the full
    hosted suite on their branch; the queue is the gate. development.md
    says which tier each kind of change runs locally.
19. **Tier timeouts:** `unit`, `recognition` and `app` warn at 30 s and are
    killed at 2 min, `app` in a test group of at most 4 threads; `renders`
    keeps 11 / 21 min and its watchdog; `scripts` by job limit. Job limits:
    fast tier 15 min, macOS desktop 30, iOS 40, Windows 30, Android 40, each
    detection-gate shard 10. #513 closes once #510's fix lands; a slow `app`
    test is a bug.
20. **Quarantine list** `.config/quarantine.txt`: `<nextest filter>
    #<issue>  <expires YYYY-MM-DD>`; CI excludes listed tests and runs them
    alone without failing; the fast tier fails on an expired line; entries
    by PR, the hub may add one.
21. **Queue fallback:** a PC job runs hosted when the PC's heartbeat is
    older than 10 min or the job has not started 10 min after the queue
    commit appeared; the first result counts and `merge-gate` names where
    it ran. ADR 0082's thresholds and per-head overflow go.
22. **Rollout:** interim now (the dated `zip` ignore; #510's fix, then close
    #513; delete the Mac CI checkout's `target/`); then (1) tiers,
    quarantine, the OS-services seam; (2) hosted jobs as parallel lanes
    (macOS desktop and bash 3.2, iOS, Android on KVM, the detection-gate
    experiment and sharding, the fast tier on Linux); (3) the `merge-gate`
    aggregator; (4) the PC runner; (5) a shadow trial on 5 PRs failing a
    broken PR per tier; (6) cutover (ruleset, queue, fleet checks, Mac
    runner retired); (7) `main`'s runs, the nightly, cargo-deny's split.
23. **ADRs:** [ADR 0095](../adr/0095-ci-on-a-merge-queue.md) supersedes ADR
    0082 and amends ADRs 0070, 0091 and 0093, each with a status line;
    ROADMAP gets a CI redesign section (items 1–7) and loses the Engineering
    items it replaces. The fleet repository's profile and hub docs are a
    follow-up there.

## Outcome

[ADR 0095](../adr/0095-ci-on-a-merge-queue.md), ROADMAP's CI redesign
section, and glossary terms (merge queue, queue commit, fast tier, test
tier, `merge-gate`, quarantine list) in [CONTEXT.md](../../CONTEXT.md).

### Round 4: the fleet side and who implements (asked and agreed 2026-11-02)

All eight recommendations accepted; the fleet side is fleet ADR 0021
("Merge queues", in the `fleet` repository, merged at the cutover).

24. **This session is the CI redesign hub** (`fleet-hubs`: `lawkitt/mdoc
    ci-redesign`): lanes for items 2a–2e within the Mac's builder cap, a
    Windows Desktop session for item 4, Codex reviews of code and script
    PRs; it merges CI redesign PRs only, run 3's hubs keep theirs.
25. **Beside run 3:** items 1–5 never change what the current gate requires;
    each CI-changing merge is announced to both hubs (a message to the Mac
    hub, a "**CI redesign hub**" comment on #471); the cutover happens only
    between run 3 waves, announced 30 minutes ahead, after both hubs answer
    "idle".
26. **Interim:** the `zip` ignore now (#519); #510's fix stays with the Mac
    hub's lane; the Mac CI checkout's `target/` deleted between jobs (done
    11:15Z: 62 GB free before, 136 GB after).
27. **Renders before the queue:** a UI lane dispatches the hosted macOS
    desktop job on its branch with a `screens` input; the job uploads those
    PNGs as a small artifact beside the full set.
28. **Fleet ADR 0021:** profile `merge_queue = true`; `checks` `["lint
    (ubuntu)", "merge-gate"]` with a fast `gate`; the hub's gate decides when
    to enqueue (`gh pr merge --auto`, never `--admin`); no one-at-a-time and
    no hand re-test; `fleet-watch` events `queued`, `dequeued`, `merged`;
    base red from the nightly or `main`'s cache builds; preflight checks the
    PC runner's heartbeat; the runner no longer counts as a Mac build slot.
29. **The config PR merges at the cutover;** after `sync-fleet.sh` both hubs
    are told to re-invoke the `fleet` skill, and this session reloads it.
30. **The maintainer approves, at the moment:** the ruleset and merge queue,
    uninstalling the Mac runner, reinstalling the PC runner in queue mode.
31. **Budget:** the same 65% weekly Opus line as run 3; Codex Sol past it;
    run 3 has priority on the Mac's builder slots; with both providers
    limited, remaining items are parked with their state written down.
