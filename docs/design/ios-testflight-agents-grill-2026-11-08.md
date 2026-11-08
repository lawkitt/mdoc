# Agents publish to TestFlight grill, 2026-11-08

The maintainer wants agents to publish the iOS app to TestFlight without
the maintainer archiving in Xcode by hand. Every TestFlight build so far
was a manual archive. Decisions are recorded per round; the ADR follows
when the tree closes.

## Facts found before round 1

- **Today's flow** ([release-run.md](../release-run.md) "iOS beta build"):
  an agent tags `ios-v<x.y.z>-rc.N` on a `main` commit (after the
  maintainer's yes) and prepares a clean full-history checkout with the
  core built; the maintainer archives it in Xcode on the macbook-air,
  distributes with "App Store Connect" and pastes What to Test.
- **The plan on paper.** ADR 0091 says releases are hosted and started by
  agents (`just release-ios`, a tag workflow, no local release path, no
  signing secret on the maintainer's machines). ADR 0097 says build 1 is
  the only manual upload and that from build 2 the workflow uploads with
  a Team API key, Admin role, for Apple-managed (cloud) signing, its
  secrets in the `release-ios` environment. ROADMAP Mobile CI/CD item 6
  tracks it. None of it was built: builds 734 and 742 and the two 0.2.0
  tags were all manual, and ADR 0097's "only manual upload" is stale.
- **Tags already on `main`:** `ios-v0.1.0-rc.1` (732, refused on the
  export key), `rc.2` (734, build 1), `rc.3` (742, build 2),
  `ios-v0.2.0-rc.1` (750, its archive failed to compile, fixed by #613)
  and `ios-v0.2.0-rc.2` (751), all tagged 2026-11-04 to 11-05. ROADMAP
  and the refactor grill (Q24) still call the next build
  `ios-v0.2.0-rc.1`, a tag that exists. `main` has 817 commits now.
- **Nothing handles `ios-v*` tags.** `release.yml` matches `v*` only.
- **The `release-ios` environment exists** and admits `main` and
  `ios-v*` only, with no reviewers and no secrets yet. `release` holds
  the desktop's Apple secrets (certificate, Apple ID, app password).
- **Hosted iOS CI exists.** `ci-ios.yml` runs on `macos-26`: `ci/ios
  (tests)` (core, app, XCTest and XCUITest, 40 min limit) and
  `ci/ios-release` (the Release configuration, unsigned, what an archive
  compiles). It is `workflow_call`-able. Xcode 26.2 is pinned in
  `scripts/tools.env`.
- **The Mac is not a CI runner** (ADR 0095); the repository has no
  self-hosted runners.
- **The project:** automatic signing, team `F33DLM8K53` (the
  maintainer's individual membership), bundle IDs
  `io.github.lawkitt.mdoc` and `io.github.lawkitt.mdoc.RestoreExtension`
  (the share extension), `MARKETING_VERSION = 0.2.0`. The build number
  and core commit come from `scripts/ios-build-info.sh` (commit count;
  `MDOC_REQUIRE_FULL_HISTORY=1` fails a shallow clone).
- **This Mac:** Xcode 26.2 with `altool` and `notarytool`; no fastlane;
  one signing identity, Apple Development (no Apple Distribution: cloud
  signing made it at each archive); Xcode is signed in to the
  maintainer's Apple ID. No App Store Connect API key in the usual
  places (`~/.appstoreconnect/private_keys`, `~/private_keys`).
- **Xcode Cloud is unavailable to the account** (the Russia/Belarus
  restriction, found 2026-11-04). Whether App Store Connect API keys are
  also restricted is not known.
- **What to Test.** `apps/ios/CHANGELOG.md` has only an empty Unreleased
  section: the fragments were never joined. `changes/ios/` holds 85
  fragments, about 39 KB (TestFlight's What to Test takes 4,000
  characters), written for the changelog, some about recognition rules.
  Build 2's What to Test was a hand-written summary of about 1 KB.
- **TestFlight setup:** the internal group "Maintainer" (the maintainer
  only) has automatic distribution; Lawyers (external) is not yet in use.
- **The device gate** (`just ios-device-test` on both phones, ADR 0097)
  needs the phones wired to the Mac, unlocked, with UI Automation on;
  only the Mac can run it.
- **`main`'s ruleset** lets the admin role bypass the merge queue; the
  desktop `just release` pushes its release commit and tag straight to
  `main`.

## Round 1

All six accepted as recommended; Q6 goes further than recommended.

1. **Builds run hosted.** Pushing an `ios-v*` tag runs a workflow on
   `macos-26` that tests the tagged commit, archives, signs and uploads
   with an App Store Connect Team key kept in `release-ios` (ADR 0091 and
   0097's plan, never built until now). No secret is on the Mac. Apple
   approved the account's App Store Connect API access (Team Keys page
   checked 2026-11-08: approved, no keys yet), so the Xcode Cloud block
   does not reach it.
2. **The next build is `ios-v0.2.0-rc.3`.** `ios-v0.2.0-rc.1` (750) and
   `rc.2` (751) exist and tags are never reused. ROADMAP iOS items 1 and
   8 are corrected.
3. **Agents push `rc` tags alone** once the gate passes, and the hub
   reports the build. These builds reach only the internal group
   "Maintainer". The maintainer keeps a new `MARKETING_VERSION`, the plain
   `ios-v<x.y.z>` tag (App Store submission), adding builds to Lawyers
   and Beta App Review.
4. **The gate is the iPhone 12 Pro every build; both phones before
   Lawyers.** The 12 Pro is a test phone that can stay wired and awake.
   The maintainer's daily iPhone 17 Pro keeps its TestFlight install:
   a device test would replace it with a development build. Replaces
   ADR 0097's "both phones before every build".
5. **What to Test is a short text an agent writes** for testers, in build
   2's style: the never-client-files line, "New in this build" (since the
   last uploaded build), "Please try", known gaps, under about 2,000
   characters, which the workflow sets on the build. (Round 3 Q15 moves it
   from a file in a release commit to the tag's annotation.) The CHANGELOG
   section is too long (about 39 KB of fragments against 4,000 characters)
   and holds rule notes testers cannot check.
6. **Internal builds have no cap.** A build goes out whenever merged work
   is worth the maintainer's look: the maintainer is often away with the
   iPhone 17 Pro and tests TestFlight builds there, sending feedback and
   screenshots through the Claude Code mobile app. ADR 0097's "at most
   weekly, only for a fix that affects testers" now applies only to
   builds sent to Lawyers.

## Round 2

All eight accepted as recommended.

7. **Apple-managed signing with an Admin Team key.** `xcodebuild
   -allowProvisioningUpdates` with the key creates and renews the
   distribution certificate and the profiles of the app and the Restore
   extension, as the manual archives did; nothing to renew by hand. Apple
   accepts only Admin for cloud signing. Only `main` and `ios-v*` tags
   can read the key, and the maintainer can revoke it at once.
8. **The key is in `release-ios`.** The maintainer generated the Team key
   and entered `ASC_KEY_ID`, `ASC_ISSUER_ID` and `ASC_KEY_P8` (all three
   present 2026-11-08, names checked only). `ASC_KEY_P8` holds the key's
   base64 body without the `BEGIN`/`END PRIVATE KEY` lines; the workflow
   adds them (and accepts a full PEM too). Agents never see or set the
   secrets, and no copy is on the Mac.
9. **The workflow (`ios-release.yml`, on an `ios-v*` tag):** check the tag
   is on `main`, its `x.y.z` is `MARKETING_VERSION` and
   `ITSAppUsesNonExemptEncryption` is NO; call `ci-ios.yml` on the tag
   (tests and the Release configuration); archive from a full-history
   checkout (`MDOC_REQUIRE_FULL_HISTORY=1`), the key in the runner's temp
   folder for that step only; export with `destination=upload`; poll the
   App Store Connect API until processing ends (fail after 45 minutes),
   set What to Test and check the build is in "Maintainer"; write a run
   summary (version, build, tag, processing state).
10. **`just release-ios <x.y.z>-rc.N <what-to-test file>`**, run by an
    agent on the macbook-air (the only machine with the 12 Pro): clean,
    current `main`; refuses a new `MARKETING_VERSION`; checks the text;
    tags with the gate in the annotation. (Round 3 Q15 drops the release
    commit and `apps/ios/WhatToTest.md`.)
11. **The hub watches the tag's run** and reports version, build and link
    once the build is processed and in "Maintainer"; TestFlight notifies
    the maintainer's phone. A runner or network failure: rerun the failed
    job once. A test, build, signing or processing failure: report, fix in
    a PR, new `rc.N`; tags are never moved or deleted. An upload Apple
    already has (a rerun after a good upload) counts as done.
12. **No local release path** (ADR 0091 stands). If hosted macOS runners
    are down, the hub retries later; the maintainer's manual Xcode archive
    stays in release-run.md as the last resort.
13. **The iPhone 12 Pro stays ready:** always wired to the Mac and
    unlocked, now with a SIM (mobile data) and Wi-Fi at times. The device
    tests answer Use cellular. If it is not reachable, the hub tells the
    maintainer and builds no `rc`, never skipping the gate.
14. **`rc.3`'s "New in this build" counts from build 742**
    (`ios-v0.1.0-rc.3`): the maintainer is not sure 751 went up, and 742
    covers everything they may not have seen. Later builds count from the
    previous `rc` tag the workflow uploaded.

## Round 3

All six accepted as recommended; the tree is closed.

15. **An `rc` build is a tag with no commit** (revisits Q5 and Q10). The
    tag goes on an existing `main` commit, the one the 12 Pro gate tested,
    so the uploaded build is the tested one and its number is that
    commit's count. The annotation's body is What to Test, with the gate
    on trailer lines below a separator; the workflow checks the opening
    line and the 4,000-character limit. No push past the queue: ADR 0095
    and 0100 keep the ruleset's bypass for the maintainer's `just release`
    and break-glass reverts, and agents never use it. A file under
    `apps/ios/` would also have cost a full iOS run per build. The
    `changes/ios/` fragments join `apps/ios/CHANGELOG.md` at the plain
    `ios-v<x.y.z>`, the maintainer's App Store step, by a normal PR.
16. **A validate mode proves the pipeline first.** `ios-release.yml` can
    be dispatched from `main` to archive, cloud-sign, export and run
    `altool --validate-app` with the key, without uploading, so pipeline
    bugs burn no `rc.N`. The implementing PR is done when a validate run
    from `main` passes.
17. **`rc.3` waits for the pipeline** and is its first real run (ROADMAP
    Mobile CI/CD item 6's done-when), unless the pipeline is not merged
    within a day of the 12 Pro gate passing; then the maintainer chooses
    one last manual archive, and the pipeline starts at `rc.4`.
18. **The iPhone 17 Pro gets mdoc from TestFlight only.** A first look
    (refactor grill Q3's Release development build) becomes an `rc`
    build; the hub installs nothing on the 17 Pro unless the maintainer
    asks.
19. **Docs in this PR:** ADR 0107 (amends ADR 0097 and ADR 0091, with
    status lines on both), release-run.md's "iOS beta build" rewritten,
    ROADMAP Mobile CI/CD item 6 rewritten and iOS items 1 and 8 naming
    `rc.3`, and a note on the refactor grill's Q24.
20. **One implementation PR after this one:** `ios-release.yml`, `just
    release-ios` and its script with a shell test, built by a session on
    the macbook-air, Codex-reviewed before it opens. The hub dispatches
    it beside its iOS work; it touches only workflow and script files.
