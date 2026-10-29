# CI speed, round 3 (interview, 2026-10-29)

Amends [ADR 0070](../adr/0070-ci-speed-and-script-hygiene.md) through
[ADR 0082](../adr/0082-local-ci.md). Brief: [handoff](ci-speed-3-handoff.md).

## Measurements (2026-10-29)

PR run 40872766748 (#224's branch), warm cache, full key match:

| Step | `test (windows-latest)`, EPYC 7763, 4 vCPU | `test (macos-26)`, M1 (Virtual), 3 vCPU |
|---|---|---|
| Setup and cache restore | 1m04s | 0m32s |
| Test build | 2m49s | 1m24s |
| `mdoc` unit tests | 76 s | 48 s |
| `tests/headless_renders.rs` | 514 s | 406 s |
| Renders artifact upload | 50 s (167 MB) | 40 s |
| Job | ~15 min | ~10.5 min |

- The same render run takes 55 s on the Ryzen 9 3900X (ADR 0070). On the M4
  it took 418 s on 2026-10-29 (cold 77 s debug build; another worktree's
  render test ran at the same time), writing 534 MB; the handoff had about
  240 s. A sample shows one thread at ~97% CPU, hot in the unoptimized
  `image` crate (readback copies and channel swaps) and `fdeflate` (PNG
  compression), which run on the main thread on macOS. Slowest screens
  (Tab walks): `setup-consent` 23 s, `settings-detection-categories` 21 s,
  `rename-case` 20 s.
- Experiment on the same M4 (not committed): `opt-level` 2 for
  `gpui-pre-apple`, `image` and `fdeflate` brought the render run from 418 s
  to 128 s (incremental rebuild 26 s), again beside another worktree's render
  test. The cold-build cost under ADR 0070's ≤30% rule is not yet measured.
- Job queueing on the busy morning of 2026-10-29 (eight runs, up to six PRs
  open): 2–5 s per job. Runners are slow, not scarce.
- Cache: 17 entries, 11.6 GB, all on `main`; old generations of each job's
  key (for example `…-796622c3`, last used 2026-10-28) push it over 10 GB.
- The last 60 PRs are all the maintainer's, from branches in `lawkitt/mdoc`.
  Fork PRs need approval for all external contributors; no self-hosted
  runners exist; the `lawkitt` organization is on the free plan (larger
  hosted runners need a paid plan, billed per minute); GitHub merge queue is
  available to public organization repositories.
- Agents already run `just lint` and `just test` on their own machine before
  they push (the fleet profile's gate), so hosted PR CI mostly repeats that
  run and adds the other OS and a clean checkout.

## Decisions

1. **Where the gate runs: local CI.** A runner script on the Windows PC and
   on the Mac tests PR heads from branches in `lawkitt/mdoc` (never forks) at
   the exact commit, in a dedicated clean CI worktree that keeps its
   `target/`, and posts `local/windows` and `local/macos` commit statuses.
   Hosted CI on PRs shrinks to the fast checks; hosted runs stay for `main`
   after merge (a clean-environment net) and for releases. Rejected:
   hosted-only speed work (ceiling about 7–9 min, WARP on 4 vCPUs);
   self-hosted Actions runners (on a public repository a fork PR can point
   the workflow at the maintainer's machines, guarded only by the approval
   click); paid larger runners (cost, and every merge still re-runs).
2. **Both shipped OSes before a merge,** for every code PR.
3. **Job timeouts.** `timeout-minutes`: 15 for lint, 20 for clippy, 30 for
   test jobs; the local runner uses the same limits.
4. **cargo-nextest now.** Per-test slow timeout: warn at 60 s, kill at
   2 min; the render test overrides it to its own watchdog (10 min macOS,
   20 min Windows). No retries, no JUnit. Doctests run with
   `cargo test --doc`. Version pinned in `scripts/tools.env`.
5. **Cache generations.** After `main` saves a cache, a step on `main` only
   (the only `actions: write` in `ci.yml`) deletes older generations of the
   same job's key. No `cargo-sweep`, `CARGO_INCREMENTAL` change or sccache.
6. **Render-file conflicts.** Scenes in `src/workspace/headless_renders.rs`
   split into one module per feature; the test's screen list moves to a plain
   one-name-per-line file read with `include_str!` and marked `merge=union` in
   `.gitattributes` (local merges only; GitHub's web merge ignores it).
7. **Local gate contents.** Each OS runs `just lint` (clippy) and `just test`
   (nextest, the render test, the release-script tests). `fmt-check`,
   `deny` and `lint-scripts` stay on the hosted `lint (ubuntu)` job.
8. **Hosted CI on PRs:** `lint (ubuntu)` only. Dependabot and fork PRs get
   the full hosted matrix and never run on the maintainer's machines (new
   third-party code is compiled only on hosted runners before review). A
   manual `workflow_dispatch` runs the full hosted matrix on any branch.
9. **Hosted CI on `main`** keeps the full matrix after each merge: it saves
   caches, uploads renders (retention 7 days, was 14) and is the
   clean-environment net. A red `main` stops merges until fixed or reverted.
10. **Runner code and trigger.** `scripts/local-ci.sh` behind `just
    local-ci`, in mdoc. It polls open non-draft PRs every 60 s with `gh`,
    keeps one queue per machine, replaces a queued or running older head of
    the same PR with its newer head, and posts `pending` then `success` or
    `failure`. It runs as a launchd agent on the Mac and a Task Scheduler
    task at logon on Windows.
11. **Windows CI checkout:** its own clone at `D:\ci\mdoc` (1 TB SSD; C: has
    31 GB free), separate from the lanes.
12. **Mac capacity.** Local CI is an extra build slot beside the two agent
    builders, at normal priority; it starts a run only while swap is under
    2 GB and at least 40 GB is free.
13. **Logs and renders.** Each machine serves its CI output folder (log,
    summary, full render set, last 7 days) to the tailnet only with
    `tailscale serve`; the status links there. The hub reads macOS renders
    from the Mac's CI folder and opens two Windows PNGs over the tailnet
    instead of downloading ~450 MB. No gists.
14. **Who waits.** An agent's Done-when is the PR open with `local/windows`,
    `local/macos` and `lint (ubuntu)` green; the agent fixes failures
    itself. The hub's merge gate reads the same three checks.
15. **Merge order.** The hub merges one PR at a time; no GitHub merge queue
    (it needs required checks in a ruleset, which would block `just
    release`'s direct push, and the runner would have to test queue
    branches). A green PR that is behind `main` but merges cleanly is not
    re-tested; `main`'s hosted run is the net. A conflict resolution is a
    new head and is tested as usual.
16. **Drafts.** Local CI skips draft PRs; agents open PRs as ready once
    their local gate passes.
17. **Machine off.** With the PC off, the hub dispatches the full hosted
    run on the PR branch; a green `test (windows-latest)` stands in for
    `local/windows`. With the Mac off nothing merges anyway.
18. **Mac render speed,** in order: `opt-level` 2 for the crates the macOS
    profile shows hot (`image`, `fdeflate`, and `gpui-pre-apple`, whose
    `read_texture_to_image` the sample attributes partly to `rand_core`
    through shared generics) under ADR 0070's ≤30% cold-build rule; the
    Windows background PNG encoder thread on macOS too; then, only if the M4
    render is still over ~90 s, shard the render test into 3–4 nextest tests
    in their own processes. The render test stays on every PR with all of
    its evidence.
19. **`release.yml`** keeps calling the full hosted `ci.yml` (with the
    timeouts and nextest); releases never depend on local CI.
20. **PRs and order.** (1) This docs PR: ADR 0082, this record, the ADR
    0070/0037 status lines, glossary, ROADMAP. (2) Hosted hygiene:
    timeouts, nextest, cache-generation cleanup, renders retention 7 days;
    lands any time. (3) Render speed and render files: macOS `opt-level`s,
    the encoder thread, then the per-feature scene split and the union
    screen list; one PR or two in a row (same files). (4) The local CI
    runner: script, `just local-ci`, launchd and Task Scheduler setup,
    `tailscale serve`, the `D:` clone, development.md. (5) Cutover: PR CI
    becomes `lint (ubuntu)` plus the Dependabot/fork matrix and
    `workflow_dispatch`, only after (4) has run on real PRs. (2), (3) and (4)
    touch different files and may run in parallel.
21. **Who.** (3): Claude in a Mac worktree. (4): Claude writes the script
    and sets up the Mac; a Codex thread on `windows-lane-1` installs and
    verifies the Windows side and fixes Windows-specific issues in the same
    PR. (2): Codex on `windows-lane-2`. Code PRs get the other provider's
    review; fleet usage is checked before dispatch.
22. **Shadow period.** After (4) merges, local statuses run beside the full
    hosted CI; (5) lands once five PRs agree on both and a deliberately
    failing push on a test branch fails locally.
23. **Credentials.** The runner uses each machine's existing `gh` login; no
    new token (ADR 0037's "no personal access token" holds).
24. **Glossary.** *Local CI*, *local status* and *CI checkout* in
    CONTEXT.md, under Releases.
25. **ROADMAP and fleet profile.** The "Windows CI timing on slow runners"
    item is removed (its run is recorded here; the miss is this grill);
    PRs (2)–(5) become Engineering items. The sccache item stays. The fleet
    profile's `renders.fetch` and `done_when` (and the fleet skill's merge
    gate) change later, after the fleet skill update in progress; the
    maintainer says when.
26. **Targets,** push to both local statuses green, empty queue, warm CI
    checkout: `local/windows` ≤ 4 min and `local/macos` ≤ 4 min; ≤ 8 min per
    OS when `Cargo.lock` or the toolchain changes. No target for hosted
    `main` or release runs (the job timeouts bound them). A macOS miss after
    the `opt-level`s and the encoder thread triggers render sharding
    (decision 18). Replaces ADR 0070's 8-minute hosted Windows target.
27. **Docs-only PRs.** The runner reads the same ignore list as `ci.yml`'s
    `paths-ignore`, kept in one place both use; a PR touching only those
    files gets `success` ("docs only, not tested") without a build.
28. **What is tested.** Local CI tests `refs/pull/N/merge` (the PR merged
    into `main` when the run starts), as hosted `pull_request` runs do, and
    posts the status on the PR head; the description names the `main`
    commit. A PR with conflicts has no merge ref and gets `failure`
    ("conflicts with main"). Decision 15 still holds: a later move of
    `main` does not re-test a clean PR.

The maintainer confirmed shared understanding on 2026-10-29.
