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
- **Code lanes:** `windows-lane-1` to `windows-lane-4` and
  `wsl-lane-1` (the `wsl` lane runs only the platform-neutral crates and the
  forks). One thread per lane, branch `<machine>/<topic>` from `origin/main`.
- **GUI runs:** a Claude desktop session the maintainer starts in the
  machine's `acceptance` account (fleet ADR 0011) runs that platform's
  checklist: `windows-desktop` on Windows, `macbook-air-desktop` on macOS. The
  account is a standard user with its own mdoc profile, case key, clone and
  `gh` login, so the maintainer's desktop and real case key stay out of the
  run. Until a machine has that account, the run uses the maintainer's
  account: mdoc maximized before the first screenshot, and window-only
  captures for anything smaller.

## Preflight (before the RC, while the maintainer is present)

The hub checks each item and reports pass/fail. On a failure it names the
work that depends on it and the maintainer chooses: fix, drop that part, or
wait. If the maintainer leaves without answering, only the independent parts
run.

- `windows` and `wsl` reachable; mdoc registered in T3 on both; the three
  lanes exist and are free (create `wsl-lane-1` if missing).
- On each GUI machine: the maintainer has started the Claude desktop session
  in the `acceptance` account, the screen is unlocked, and the maintainer
  writes "desktop approved" in the hub thread.
- The milestone and labels the plan uses exist on GitHub.
- A test alert from the hub reaches the maintainer's phone.

## Cutting the RC

- Before the RC: `ROADMAP.md` "Current status" is up to date (ADR 0059).
- `just release <version> --yes --skip-checks-if-ci-green`: no prompts, and
  no local checks when `main`'s CI is green (see [Releasing](development.md#releasing)).
  Never answer the prompts by piping input.
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
- Models: a full acceptance run downloads them for real and refreshes a model
  cache in the `acceptance` account, outside the mdoc profile. A patch smoke
  run copies that cache into its fresh profile instead.
- Grant mdoc to computer use by its installed `mdoc.exe` or `mdoc.app`, and
  check the granted path against the install location before the first row:
  with a second install on the machine, the name alone can resolve to the
  wrong build.
- Rows needing a drag from, or a context menu in, File Explorer or Finder
  (P3–P7): **Pending (maintainer)**. Claude's computer use gets those apps
  click-only. For P7, passing the file's path to the running app is a
  supporting check, not a Pass.
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
the rule above), fix the must-fix Fails, merge the held product PRs and bump
the forks. Re-check each fix on a local unsigned `just package` build on the
acceptance machines, only the rows it touches, then tag the stable and give it
a smoke run. Cut another RC instead only when a fix touches the installer,
signing or notarization, the update check or in-app update, or the case key
in the keychain ([ADR 0083](adr/0083-fewer-release-candidates.md)).
Publishing: a patch or stable publishes once both agent runs are done with no
must-fix Fail; rows only the maintainer can do may follow, listed in the
report.

## iOS beta build

A TestFlight build of the iOS app ([ADR 0097](adr/0097-ios-testflight-beta.md),
[ADR 0107](adr/0107-agents-publish-ios-betas.md)). An agent on the
macbook-air does each step unless it names the maintainer.
Until ROADMAP Mobile CI/CD item 6 lands `ios-release.yml` and `just
release-ios`, steps 4 and 5 are the last resort below, after the
maintainer's yes to the tag.

1. **When.** Whenever merged work is worth the maintainer's look; no cap
   for the internal group. A build for Lawyers: at most weekly, for a fix
   that affects testers.
2. **Gate.** `just ios-device-test <12 Pro UDID>` on the `main` commit to
   be tagged (the iPhone 12 Pro stays wired and unlocked). Not reachable:
   tell the maintainer and tag nothing. Before Lawyers get a build, also
   the detector tier on the iPhone 17 Pro, which otherwise gets mdoc from
   TestFlight only.
3. **What to Test.** Write it for testers in a file outside the
   repository: the never-client-files line first, then what is new since
   the previous uploaded build (from the `changes/ios/` fragments merged
   since; a recognition change in a line or none), what to try and the
   known gaps; under about 2,000 characters.
4. **Tag.** `just release-ios <x.y.z>-rc.N <file>` tags that commit with
   What to Test and the gate in the annotation and pushes the tag; no
   commit. `x.y.z` is `MARKETING_VERSION`; a new version is the
   maintainer's. Tags are never moved or reused.
5. **Watch.** The tag's `ios-release.yml` run tests, archives, signs,
   uploads, waits for processing and sets What to Test. Report version,
   build and the run link once the build is in "Maintainer". A runner or
   network failure: rerun the failed job once. Any other failure: report
   it, fix it in a PR, then a new `rc.N`.
6. **Export compliance.** Every build carries
   `ITSAppUsesNonExemptEncryption` NO and the workflow checks it (App
   Store Connect's answers: standard algorithms in addition to Apple's,
   not in France; the App Store in France stays off).
7. **Internal acceptance.** The maintainer installs from TestFlight;
   section 13 of [platform acceptance](platform-acceptance.md) goes to
   `docs/evidence/mobile/<date>/ios.md` when a build is up for acceptance
   (P12 in the iPad Simulator, marked "Simulator only", until an iPad run).
8. **External** (maintainer): once section 13 and both phones' device runs
   have passed, add the build to the Lawyers group; the first external
   build of a version goes through Beta App Review with the notes below.
9. **App Store version** (maintainer): the plain `ios-v<x.y.z>`, with the
   `changes/ios/` fragments joined into `apps/ios/CHANGELOG.md` by a PR.

**Last resort** (hosted macOS runners unusable for long): the maintainer
archives a clean full-history checkout of the tag in Xcode
(`MDOC_REQUIRE_FULL_HISTORY=1 just ios-core` first) and distributes it with
"App Store Connect", never "TestFlight Internal Only", then pastes What to
Test from the tag.

**Beta description:**

> Test with documents you are allowed to share, never client files; check a
> screenshot before you send it. mdoc prepares documents for AI apps on your
> iPhone or iPad: it reads a PDF, Word file, photo or scan, finds the names,
> replaces them with aliases before you share the text, and puts the names
> back into the AI's reply. Nothing is uploaded.

**What to Test, build 1** (kept as an example of the tone):

> Test with documents you are allowed to share, never client files; check a
> screenshot before you send it. Try: + → Files, Scan or Paste; Find names;
> tap an underlined name to keep or replace it; Share to ChatGPT or Claude;
> share the reply back with "Restore with mdoc". Known gaps: phones with less
> than 8 GB of memory (before the iPhone 15 Pro) cannot find names and mark
> them by selecting text; English interface only; not yet checked on a real
> iPad.

**Beta App Review notes:**

> mdoc needs no account or sign-in. It works offline; the only network use is
> optional model downloads the user starts (text recognition and a name
> detector, about 650 MB in all), fetched from pinned URLs and checked by
> SHA-256. They are model data, not executable code. Phones with less than
> 8 GB of memory cannot run the name detector by design; there the app says
> so and names are marked by selecting text and choosing Replace…. To try it:
> tap +, choose Paste and paste the paragraph below, tap Find names
> (download the detector when asked, on Wi-Fi), then Share; in the AI app,
> share a reply back to "Restore with mdoc".
>
> "This lease is made between Anna Petrova, of 12 Garden Street, Springfield,
> and John Miller, acting for Northwind Holdings LLC. Anna Petrova will pay
> John Miller the rent on the first day of each month."

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
