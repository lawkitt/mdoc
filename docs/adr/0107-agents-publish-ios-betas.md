# Agents publish iOS betas

Status: Accepted, 2026-11-08. Amends [ADR 0097](0097-ios-testflight-beta.md)
(manual uploads, the gate, cadence, What to Test) and
[ADR 0091](0091-mobile-ci-and-releases.md) (the iOS release recipe and
workflow). Record:
[agents TestFlight grill](../design/ios-testflight-agents-grill-2026-11-08.md).

## Problem

Every TestFlight build so far was archived in Xcode by the maintainer:
734 and 742 for 0.1.0, then the `ios-v0.2.0-rc.1` and `rc.2` tags. ADR
0097 planned a hosted workflow from build 2 and ADR 0091 a `just
release-ios` recipe, but neither was built and nothing runs on `ios-v*`
tags. The maintainer wants agents to publish builds alone, often enough
to test new work on the iPhone 17 Pro while away. ADR 0097's gate (both
phones every build) and cadence (at most weekly) assume rare builds; a
release commit per build would push past the merge queue, which agents
never do (ADR 0095); and the iOS changelog fragments are about ten times
What to Test's 4,000 characters.

## Decision

### Where and how

- **Hosted, on the tag.** `.github/workflows/ios-release.yml` runs on an
  `ios-v*` tag on `macos-26`:
  1. checks the tag is on `main`, its `x.y.z` is `MARKETING_VERSION` and
     `ITSAppUsesNonExemptEncryption` is NO;
  2. calls `ci-ios.yml` on the tag (tests and the Release configuration);
  3. archives the Release configuration unsigned from a full-history
     checkout (`MDOC_REQUIRE_FULL_HISTORY=1`), then signs the archived app
     and Restore extension ad hoc with their entitlements;
  4. exports with `destination=upload`, `-allowProvisioningUpdates` and
     the API key: Apple-managed signing replaces the ad hoc signatures;
  5. polls the App Store Connect API until processing ends (fails after
     45 minutes), sets What to Test and checks the build is in the
     internal group "Maintainer";
  6. writes a run summary: version, build, tag, processing state.
- **Validate mode.** The workflow can be dispatched from `main` to run
  step 1 (on `main`'s tip), step 3, an export to a file and `altool
  --validate-app`, without uploading, so a pipeline fault burns no `rc.N`.
  It skips step 2: `main`'s commits passed the merge queue's tests.
- **No local release path** (ADR 0091 stands). If hosted macOS runners are
  down, the hub retries later. The maintainer's manual Xcode archive stays
  in [release-run.md](../release-run.md) as the last resort.

### Signing and the key

- **Apple-managed signing with a Team key, Admin role** (as ADR 0097): the
  key makes and renews the distribution certificate and the profiles of
  the app and the Restore extension. Apple accepts only Admin for cloud
  signing.
- **No certificate on the runner.** The archive is unsigned and then
  signed ad hoc, because an unsigned archive exports without its
  entitlements (App Group, keychain group, increased memory limit) and
  Xcode's automatic signing at archive time makes a development
  certificate on every fresh runner. Checked on 2026-11-08: the export
  gives "Apple Distribution" signatures with every entitlement.
- **The key lives in `release-ios` only**: `ASC_KEY_ID`, `ASC_ISSUER_ID`
  and `ASC_KEY_P8`, entered by the maintainer on 2026-11-08. `ASC_KEY_P8`
  holds the key's base64 body without the `BEGIN`/`END PRIVATE KEY`
  lines; the workflow adds them (and accepts a full PEM). The key file
  exists in the runner's temp folder for the archive and upload steps
  only. Only `main` and `ios-v*` tags can read the environment.

### An `rc` is a tag

- **`just release-ios <x.y.z>-rc.N <what-to-test file>`**, run by an agent
  on the macbook-air (the only machine with the gate's phone), tags an
  existing `main` commit: the one the gate tested (`origin/main`'s tip,
  or an older one named with `--commit`). It refuses a checkout with
  changes or untracked files, a stale HEAD and a version other than
  `MARKETING_VERSION`, makes no
  commit, and pushes only the annotated tag. Its build number is that
  commit's count on `main` (ADR 0091).
- **The annotation carries What to Test.** Subject: `mdoc for iOS <x.y.z>
  rc.N (build <n>)`. Body: What to Test, opening with "Test with documents
  you are allowed to share, never client files; check a screenshot before
  you send it.", then what is new since the previous uploaded build, what
  to try and the known gaps, under about 2,000 characters. Below a `---`
  line, trailers record the gate (phone, commit, result). The recipe and
  the workflow both check the opening line and the 4,000-character limit.
- **An agent writes What to Test** from the `changes/ios/` fragments merged
  since the previous uploaded build, for testers rather than for the
  changelog (recognition-rule details become one line or none). `rc.3`
  counts from build 742 (`ios-v0.1.0-rc.3`).
- **The changelog joins at the plain tag.** The fragments join
  `apps/ios/CHANGELOG.md` when the maintainer releases `ios-v<x.y.z>` for
  App Store review, by a normal PR. This replaces ADR 0097's "from build 2
  the version's CHANGELOG section fills What to Test" and, for `rc`
  builds, ADR 0091's "set the version, commit, tag".
- **Next tag:** `ios-v0.2.0-rc.3`. `rc.1` (750) and `rc.2` (751) exist,
  and tags are never reused or moved.

### Who decides

- **Agents push `rc` tags alone** once the gate passes; the hub reports
  each build. `rc` builds reach only "Maintainer".
- **The maintainer** sets a new `MARKETING_VERSION`, releases the plain
  `ios-v<x.y.z>`, adds builds to Lawyers and answers Beta App Review.

### Gate and cadence

- **The iPhone 12 Pro gates every build** (`just ios-device-test`, the
  below-floor tier), on the commit to be tagged, from a clean checkout
  (untracked files count). The test records the pass with the phone's
  real model, and only a phone below the floor counts, never a forced
  tier on a newer phone. It stays wired to the Mac
  and unlocked, with mobile data and Wi-Fi at times. If it is not
  reachable, the hub tells the maintainer and tags nothing; the gate is
  never skipped.
- **Both phones gate a build before Lawyers get it** (ADR 0097's detector
  tier on the iPhone 17 Pro).
- **The iPhone 17 Pro gets mdoc from TestFlight only.** A device test or
  development build there would replace the TestFlight install, so the
  hub installs nothing on it unless the maintainer asks; a first look at
  new work is an `rc` build.
- **Internal builds have no cap**: one goes out whenever merged work is
  worth the maintainer's look. ADR 0097's "at most weekly, only for a fix
  that affects testers" now applies to builds sent to Lawyers.

### Results and failures

- The hub watches the tag's run and reports version, build and run link
  once the build is processed and in "Maintainer"; TestFlight notifies
  the maintainer's phone.
- A runner or network failure: the hub reruns the failed job once. A
  test, build, signing or processing failure: the hub reports it, a PR
  fixes it, and the next build is a new `rc.N`. An upload Apple already
  has (a rerun after a good upload) counts as done.

## Invariants

- The App Store Connect key is only in `release-ios`; it never passes
  through chat, the repository, files on the maintainer's machines or an
  agent. Only the maintainer enters or revokes it.
- Every uploaded build is a public tag on a `main` commit that hosted CI
  and the 12 Pro gate tested; agents push no commit to `main` to make one.
- Nothing reaches testers other than the maintainer without the
  maintainer (ADR 0091, ADR 0097).

## Consequences

- An Admin key can manage users and every app in the account; a leak
  through the workflow would matter beyond mdoc. Its exposure is the
  environment's tag and branch rule and the workflow's own steps.
- Each build costs a hosted iOS run (about 40 minutes) and an Apple
  processing wait, and the 12 Pro gate on the Mac.
- `apps/ios/CHANGELOG.md` stays empty through a beta; a tester's view of
  changes is the run of What to Test texts in the tags.
- Order of work: this record; one implementation PR (`ios-release.yml`,
  `just release-ios` and its script with a shell test), done when a
  validate run from `main` passes; then `ios-v0.2.0-rc.3` as the first
  real run. If the pipeline is not merged within a day of `rc.3`'s gate
  passing, the maintainer may archive `rc.3` by hand and the pipeline
  starts at `rc.4`.

References: [interview](../design/ios-testflight-agents-grill-2026-11-08.md).
