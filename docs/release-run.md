# Release runs

Standing rules for cutting a release candidate and running its platform
acceptance in parallel on the maintainer's machines while the maintainer may be
away (fleet ADR 0010, the `fleet` skill's `dispatch.md`). Each run's prompt
names only that run's choices (see the end of this file); everything else
comes from here, [Fleet](development.md#fleet), [Releasing](development.md#releasing)
and the [acceptance checklist](platform-acceptance.md).

## Roles

- **Hub:** a Claude T3 thread on `macbook-air`. It owns the root checkout
  `~/Developer/lawkitt/mdoc` on `main`, cuts the RC, dispatches the lanes and
  GUI runs, polls GitHub, assembles the evidence and reports. Every other
  session on the Mac, including a Claude desktop session the maintainer
  starts, works in its own worktree.
- **Code lanes:** `windows-lane-1`, `windows-lane-2` and `wsl-lane-1` (the
  `wsl` lane runs only the platform-neutral crates and the forks). One thread
  per lane, branch `<machine>/<topic>` from `origin/main`.
- **GUI runs:** `windows-desktop` (Codex through T3) runs the Windows
  checklist; `macbook-air-desktop` (Codex through T3, or a Claude desktop
  session the maintainer starts) runs the macOS checklist.

## Preflight (before the RC, while the maintainer is present)

The hub checks each item and reports pass/fail. On a failure it names the
work that depends on it and the maintainer chooses: fix, drop that part, or
wait. If the maintainer leaves without answering, only the independent parts
run.

- `windows` and `wsl` reachable; mdoc registered in T3 on both; the three
  lanes exist and are free (create `wsl-lane-1` if missing).
- On each GUI machine: the desktop tool answers one harmless accessibility
  read (no screenshot), and the screen is unlocked. A desktop session the
  maintainer starts: the maintainer writes "desktop approved" in the hub
  thread.
- The milestone and labels the plan uses exist on GitHub.
- A test alert from the hub reaches the maintainer's phone.

## Cutting the RC

- Before the RC: `ROADMAP.md` "Current status" is up to date (ADR 0059).
- `just release <version>`, confirming the push. Once the non-interactive
  flags exist (ROADMAP), use them instead of answering prompts by piping input.
- The GitHub Release stays a **draft**; only the maintainer publishes. Check
  that the `.dmg` and the NSIS `.exe` are on the draft. If CI fails: fix, cut
  the RC again, and say so in the report.
- While a GUI run is using the Mac (it renames the real app-data folder), no
  mdoc builds or tests on the Mac: code work goes to the lanes.

## Lanes

- Each task: promote its roadmap item into `ROADMAP.md` when it starts, with
  a short *Done when* (ADR 0059).
- Push after every green commit; open a draft PR at the first commit; mark it
  ready when done. Claude lanes get a Codex review of the diff before the PR
  is marked ready (fleet `codex.md`).
- Test-only and CI-only PRs merge when green. Product changes (app code,
  forks) are held: open the PR, but don't merge it or bump a fork rev until
  both acceptance runs finish.
- Usage limit on a lane: the hub may start the other provider on that lane
  only to push, open or update PRs and see CI through.

## Acceptance runs

- Follow [platform-acceptance.md](platform-acceptance.md) on the RC's draft
  artifacts, from a clean profile. Never delete the case key.
- The real model download happens on `windows`; the Mac may copy models from
  the maintainer's profile when the run's prompt says so.
- Rows needing a physical keyboard, spoken screen-reader output, passwords or
  System Settings: **Pending (maintainer)**, collected into one list.
- A Fail is recorded, gets an issue in the release's milestone, and is linked;
  the run goes on. No fixes during the run.
- **Evidence:** each run commits only its own
  `docs/evidence/platform/<date>/<platform>.md`, on its own branch; it never
  touches the other platform's file. The hub writes the folder's `README.md`
  (build, machines, counts, Fail, Unavailable and Pending lists) and opens one
  PR.

## Gates

Every gate in a run's plan names its fallback. When it names none: go ahead
without the missing step, move it to the next RC, and say so in the report.
Default gate: when both acceptance runs are done (or one never started, by
the rule above), fix the must-fix Fails, merge the held product PRs, bump the
forks, cut the next RC as a draft, and list the rows to re-run per platform.

## Waiting and reporting

- The hub ends its turn and wakes on a `schedule_task` bound to its thread
  about every 10 minutes until the run is done; each wake-up polls GitHub (PRs,
  branches, CI) and reads a worker thread only when its PR changes or it goes
  quiet. Alerts reach the maintainer through T3 Connect.
- Alert when blocked, when a gate falls back, and at the end.
- Final report: PR links, what merged, what is held, the RC's state, and the
  consolidated Pending (maintainer) list per platform.

## A run's prompt

Names only what this run decides:

- The version to cut, and anything to fix before it.
- What each lane does (roadmap items by title), and which PRs merge when green.
- Which acceptance runs happen, on which machines, and any rows added or
  skipped (for example rows deferred from an earlier release, or "rerun only
  the rows rc.N's changes affect").
- Gates that differ from the default, with their fallbacks.
