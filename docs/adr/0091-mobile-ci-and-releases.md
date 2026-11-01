# Mobile CI and releases

Status: Accepted, 2026-11-01. Amends [ADR 0037](0037-ci-and-release-pipeline.md)
(pipeline), [ADR 0039](0039-macos-signing-and-notarization.md) (the
`release` environment), [ADR 0072](0072-changelog-fragments.md)
(fragments), [ADR 0082](0082-local-ci.md) (statuses, fallback, `main`),
[ADR 0085](0085-mobile-apps.md) (repository and CI, versions) and
[ADR 0089](0089-android-app.md) (local CI, signing key, distribution).

## Problem

The pipeline was built for desktop. The iOS and Android apps were added to
local CI only: the Mac's `local/macos` runs the iOS job and the PC's
`local/windows` the Android job, inside the desktop statuses. Nothing hosted
ever builds an app, so a break on `main`, a Dependabot bump of Gradle or of
the core, and the hosted fallback all go untested for mobile, while the
fallback posts the desktop status green. There is no mobile release at all:
nothing signs, uploads or versions the apps, the `release` environment
admits any branch, and mobile notes ship in the desktop CHANGELOG. The
maintainer wants CI and releases that agents run while the maintainer is
away for hours, reliable and fast; hosted runners slowed the PR gate before
(ADR 0082). Findings: [mobile CI grill](../design/mobile-ci-grill-2026-11-01.md).

## Decision

### Where tests run

- **Areas.** One path table in the repository splits changes into areas:
  **desktop** (`src/`, `tests/`, desktop scripts), **core** (`crates/`,
  `Cargo.lock`), **ios** (`apps/ios/`, the iOS scripts) and **android**
  (`apps/android/`, the Android scripts). Both local runners and `ci.yml`
  follow it, and a test fails when a copy disagrees with it. A core change
  runs every area; a Swift-only PR runs no desktop test or Android job; an
  Android-only PR runs no iOS job. This replaces the iOS job's `apps/`
  filter and the Android job's own list.
- **Statuses.** `local/ios` (the Mac) and `local/android` (the PC) join
  `local/macos` and `local/windows`; a PR outside an area gets an immediate
  `success` "not affected". The merge gate is five checks: the four local
  statuses and `lint (ubuntu)`. The desktop statuses no longer wait for a
  mobile job.
- **Mobile tests stay local.** Hosted CI runs no app tests on `main` or on
  the maintainer's PRs (ADR 0082's reason stands). The exceptions are the
  fallback, Dependabot and fork PRs, and release runs, below.
- **Fallback order: the home machine, the other machine, then hosted.**
  Android: the PC; after 15 minutes stuck (ADR 0082's thresholds), the
  Mac's runner takes `local/android` if its memory and disk gate passes;
  after 45, the hosted fallback runs the hosted Android job. iOS: the Mac;
  after 45 minutes, the hosted iOS job. A result posted by the takeover or
  the fallback names where it ran; a runner that comes back replaces it,
  as ADR 0082 says.
- **Hosted mobile jobs.** iOS on `macos-26`: the core, the app, XCTest and
  XCUITest on the pinned Simulator. Android on `ubuntu-latest` with KVM
  (hosted arm64 macOS runners have no nested virtualization): the core, the
  JVM tests, the app and the emulator tests. They run as the fallback, on
  Dependabot and fork PRs in their areas (third-party code is still never
  compiled locally before review), and in release runs.
- **Android scripts run on Windows, macOS and Linux.** This replaces ADR
  0089's "every recipe runs in Git Bash" with "and on macOS and Linux".
  The Mac's emulator runs `arm64-v8a`, the shipped ABI; the PC's runs
  `x86_64`.
- **Local runners test `main`.** After its PR queue empties, each runner
  tests each new `main` commit in the areas that commit touched and posts
  its statuses there; a red one stops merges, as a red hosted `main` does.
- **Minified Android build in local CI.** When the android area changes,
  the Android job also builds the minified release-like variant, so R8
  removing classes UniFFI loads at run time fails the PR.
- **Pinned toolchains.** `scripts/tools.env` pins the Xcode version
  (`XCODE_VERSION`) and the iOS Simulator runtime (`IOS_SIM_RUNTIME`); the
  device stays the recipes' default, the iPhone 17 (`tools.env` values
  carry no spaces). `just doctor` reports a mismatch and the iOS job fails
  on one with a clear message, without failing desktop jobs; hosted macOS
  jobs select the same Xcode. Updating Xcode is its own PR. The Android emulator image stays
  pinned through `sdkmanager`. The Android `cargo check` in `just mobile`
  targets API 31, the apps' minimum.

### Dependencies

- **Dependabot PRs run the hosted jobs of their areas**: a Gradle bump the
  Android job, a `Cargo.lock` bump both mobile jobs beside the desktop
  matrix.
- **Gradle verification metadata is regenerated on a hosted runner and
  reviewed.** A manually dispatched workflow regenerates
  `apps/android/gradle/verification-metadata.xml` on a given branch with
  `contents: read` and uploads the diff as an artifact. An agent checks that
  only the bumped packages and their checksums changed and commits the file
  to the Dependabot branch, which keeps running hosted CI only. No
  `pull_request_target`.

### Releases

- **Hosted, started by agents.** `just release-ios <version>` and `just
  release-android <version>` set the version, commit, tag `ios-v<version>`
  or `android-v<version>` from `main` and push. The tag's workflow checks
  that the tag is on `main` and matches the repository's version, runs the
  hosted mobile job (nothing else tests `main` for mobile on GitHub), then
  the release-configuration tests, then builds, signs and uploads. The
  agent watches the run and reruns a job that failed on the runner. There
  is no local release path.
- **Release configurations are tested before upload**: the parity and UI
  tests on the iOS Release configuration in the Simulator, and on a
  minified, release-like Android variant on the hosted emulator.
- **iOS signing is managed by Apple.** `xcodebuild -allowProvisioningUpdates`
  with an App Store Connect API key creates and renews the distribution
  certificate and the profiles of the app and the Restore extension. The
  key's role is the least that cloud signing accepts, confirmed at setup.
  The first TestFlight build is uploaded by hand from Xcode, which creates
  the app record; the workflow follows it.
- **Google holds the Android app signing key** (Play App Signing, a key
  Google generates). CI holds only a resettable upload key. The workflow
  uploads the AAB to Play's internal track, downloads the universal APK
  Play signs from it (`generatedapks`), and attaches that APK, SHA256SUMS
  and build provenance to a draft GitHub Release, so both channels carry
  one signature. This replaces ADR 0089's maintainer-held key. If the Play
  account is not ready for `android-v0.1.0`, that release uses a
  maintainer-held key in `release-android` beside a separate upload key and
  moves to Play's key afterwards.
- **Versions.** The user-visible version (`MARKETING_VERSION`,
  `versionName`) lives in the repository, set by the release recipe and
  checked against the tag. The build number (`CURRENT_PROJECT_VERSION`,
  `versionCode`) is the number of commits on `main` up to the tagged commit:
  it only grows and needs no stored counter; an upload Apple or Play has
  already received is redone from a new commit. Each app records the core
  commit (an Info.plist key, a `BuildConfig` field) and shows it in
  Settings and crash reports.
- **Agents release up to the draft.** A run ends with the build on
  TestFlight's internal testers, on Play's internal track and in a draft
  GitHub Release. The maintainer adds external TestFlight testers, submits
  for App Store review, promotes Play's closed and production tracks and
  publishes the GitHub Release.
- **Release notes.** The version's section of the app's CHANGELOG fills
  the draft GitHub Release, TestFlight's What to Test, and Play's internal
  release notes (cut to Play's 500 characters with a link to the GitHub
  Release). The App Store's What's New and Play's production notes are
  written at submission.
- **Real-device tests stay by hand** (`just ios-device-test`, `just
  android-device-test`) before every release after the first, as a step in
  [release-run.md](../release-run.md). No scheduled device runs.
- **Environments.** Each secret set has its own environment: `release`
  (desktop), `release-ios`, `release-android`. Each admits only its tags
  (`v*`, `ios-v*`, `android-v*`) and `main`, with no required reviewers,
  so nothing waits on a click. A signed desktop test build is dispatched
  from `main`. No platform's workflow can read another's keys.

### Changelogs

- **One changelog per app.** Fragments in `changes/ios/` and
  `changes/android/` feed `apps/ios/CHANGELOG.md` and
  `apps/android/CHANGELOG.md` at that app's release; `just release` takes
  only the top-level `changes/*.md` for desktop. A core change users see on
  both platforms gets a fragment in each folder it affects.

## Invariants

- No PR merges without a real result for each affected area: a local run,
  a takeover or a hosted fallback, never a waiver.
- Third-party code reaches the maintainer's machines only after review:
  Dependabot and fork PRs build on hosted runners only, and Gradle
  verification metadata is never regenerated locally for an unreviewed
  bump.
- No signing secret or upload key is on the maintainer's machines or
  readable from a feature branch; agents start releases by pushing a tag.
- Nothing a user can install is published without the maintainer:
  agents stop at internal tracks and drafts.

## Consequences

- The fleet profile's required `checks` grows from three to five when the
  new statuses go live (fleet ADR 0017).
- The Mac runner may run Android jobs as a takeover, adding emulator load
  to the busiest machine. Memory and disk alone are not enough of a gate:
  on 2026-11-01 an Android emulator (about 440% CPU, load average above
  200) beside two agent builds timed out seven GPUI workspace tests at
  nextest's 2-minute limit in `local/macos`. The takeover therefore also
  waits for CPU load, and the Mac's runner never starts a desktop test
  while its own emulator runs.
- The hosted mobile jobs run rarely, so they can break unnoticed between
  fallbacks and releases; Dependabot PRs exercise them weekly.
- An Android release depends on Google Play being reachable; a GitHub APK
  cannot ship while Play is down or the account is suspended.
- Rollout, in order: (1) the environments, the API 31 check and the Xcode
  pin; then three parallel lanes on separate files: (2) areas, the two
  statuses and testing `main` (`local-ci.sh`, `ci.yml`, the check
  scripts' filters), (3) the Android scripts on macOS and Linux and the
  minified build (the Android scripts and Gradle), (4) the per-app
  changelogs, before the next desktop release (`release.sh`, `changes/`);
  then (5) the Mac takeover, the hosted mobile jobs, Dependabot and the
  metadata workflow, which build on 2 and 3; (6) the iOS release workflow,
  after the first TestFlight build; (7) the Android release workflow,
  after the Play account. ROADMAP's Mobile CI/CD section tracks them.

References: [interview](../design/mobile-ci-grill-2026-11-01.md).
