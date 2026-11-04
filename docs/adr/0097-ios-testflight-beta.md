# iOS TestFlight beta

Status: Accepted, 2026-11-03; amended 2026-11-04 (the export key is NO, and
build 1 is `ios-v0.1.0-rc.2`). Amends [ADR 0091](0091-mobile-ci-and-releases.md)
(mobile tags and the first build) and details [ADR 0085](0085-mobile-apps.md)'s
Distribution bullet and [ADR 0088](0088-ios-on-real-devices.md)'s first-build
gate.

## Problem

The iOS app has run only on the maintainer's two iPhones through development
signing. ROADMAP iOS 0.1 item 1 puts it on TestFlight, but nothing said which
account sells it, how beta builds are versioned and tagged when iOS's version
string cannot hold `-rc.N`, how to answer export compliance for a core that
encrypts cases with its own AES-256-GCM, what the upload is missing (a
privacy manifest, the encryption key, the core commit), which checks gate
the first build and the first outside testers, or how lawyers test a privacy
tool without sending client documents through feedback. Findings:
[TestFlight grill](../design/testflight-grill-2026-11-03.md).

## Decision

### The app record

- **Seller: the maintainer's individual membership** (team `F33DLM8K53`),
  so TestFlight and the App Store show the maintainer's name. An
  organization (D-U-N-S, a legal entity) is revisited only if one exists.
- **The record is made by hand in App Store Connect** before any upload:
  Apple ID 6830176855, name **"mdoc: private docs for AI"** (the home-screen
  name stays `mdoc`), bundle ID `io.github.lawkitt.mdoc`, SKU `mdoc-ios`,
  English (U.S.), not offered on Apple silicon Macs or Apple Vision Pro.
  Agents never sign in to App Store Connect.

### Builds and versions

- **Beta builds are `ios-v<x.y.z>-rc.N` tags.** `MARKETING_VERSION` stays
  `x.y.z` through the beta; the build number is the number of commits on
  `main` up to the tagged commit (ADR 0091) and grows with each build. The
  plain `ios-v<x.y.z>` tag marks the build submitted to App Store review.
  This replaces ADR 0091's "tag `ios-v<version>` matching the repository's
  version" for betas: an `-rc.N` tag matches when its `x.y.z` does.
- **Build 1 (`ios-v0.1.0-rc.2`) is the only manual upload.** (`rc.1` was
  tagged and archived, and Apple refused it: see the export key below.) An agent
  prepares a clean checkout of the tagged `main` commit; the maintainer
  archives it in Xcode on the macbook-air, signed in with their Apple ID,
  and distributes with "App Store Connect" (never "TestFlight Internal
  Only", which cannot reach external testers). Apple-managed signing makes
  the distribution certificate in the cloud. From build 2 the hosted
  workflow (ADR 0091 rollout item 6) uploads.
- **The workflow's key** is an App Store Connect Team key with the Admin
  role, the least that cloud-managed signing accepts per Apple's
  documentation (ADR 0091's "confirmed at setup"). The maintainer enters its
  issuer ID, key ID and `.p8` directly as secrets of the `release-ios`
  environment.
- **New builds** go out only when a fix affects testers, at most weekly,
  each after `just ios-device-test` on both phones (the detector tier on the
  iPhone 17 Pro, below the floor on the iPhone 12 Pro).

### What the upload carries

- **A privacy manifest** (`PrivacyInfo.xcprivacy`) in the app and in the
  Restore extension: no tracking, no collected data, and the required-reason
  APIs they use (`UserDefaults` for the app's own and the App Group's
  settings; file timestamps if the core's `stat` calls need declaring).
- **`ITSAppUsesNonExemptEncryption = NO`** (amended 2026-11-04; was YES).
  Cases are AES-256-GCM through the core's `aes-gcm` crate, not Apple's OS:
  a standard algorithm "in addition to" Apple's. For that App Store
  Connect's encryption questions ask only for a French (ANSSI) declaration,
  and only for the App Store in France, which stays off until one is filed.
  Answered there on 2026-11-04 ("Based on your answers, you don't need to
  upload any documents"), it says to declare no non-exempt encryption in
  Info.plist. YES with no documentation on file failed the upload of
  `rc.1` ("Invalid Export Compliance Code"). Offering France later means YES
  again, with the declaration and the code App Store Connect then issues.
  The core keeps its encryption; no CryptoKit copy in Swift.
- **The core commit** as an Info.plist key, shown in Settings › About, whose
  source link opens that commit. With the public tag per build, that is how
  each build's Corresponding Source stays available
  ([ADR 0086](0086-app-store-licence-permission.md)).
- **Privacy label "Data Not Collected"**, with `docs/privacy.md`'s iOS
  section as the privacy policy URL.

### Gates

- **Build 1, internal (the maintainer only):** PR #576 merged, ROADMAP iOS
  item 3's full device run passing on the iPhone 17 Pro, and P17 by hand on
  the 17 Pro on a development build of the same commit. Then all of
  [platform acceptance](../platform-acceptance.md) section 13 runs on the
  TestFlight build itself.
- **External testers:** section 13 passed on a TestFlight build and ROADMAP
  iOS item 4 (below the floor on the iPhone 12 Pro) passed.
- **iPad:** for TestFlight, P12 runs in the iPad Simulator and is marked
  "Simulator only"; App Store submission waits for a run on a real iPad.

### Testers

- **One external group, "Lawyers":** 5 to 10 people the maintainer invites
  by email; no public link while names can still be missed. The first
  external build of each version goes through Beta App Review; the
  maintainer adds testers, as ADR 0091 says.
- **Never client files.** The beta description and every What to Test open
  with: "Test with documents you are allowed to share, never client files;
  check a screenshot before you send it." Feedback goes through TestFlight
  only (screenshot feedback and the crash reports testers choose to share);
  the feedback email shown to testers is the maintainer's choice.
- **Beta App Review notes** say: no account or sign-in; the optional
  downloads are model data from pinned URLs checked by SHA-256, not code;
  phones under 8 GB cannot find names by design and mark names by
  selection; and a short path (+ → Paste a fictional paragraph given in the
  notes → Find names → Share → Restore with mdoc). They live in
  `docs/release-run.md`.
- **What to Test:** written by hand for build 1 (the flow, the known gaps,
  the line above); `changes/ios/` joins `apps/ios/CHANGELOG.md` at build 1's
  tag, and from build 2 that version's section fills it (ADR 0091).

### Android

Play's closed testing follows the same pattern when Android gets there:
`android-v<x.y.z>-rc.N` tags with a fixed `versionName` and `versionCode`
from the commit count, one invite-only "Lawyers" track, the same "never
client files" line, and agents stopping at the internal track.

## Invariants

- No signing certificate, API key or password is in the repository, chat,
  files or remote URLs; only the maintainer enters them, in Xcode or in the
  `release-ios` environment.
- Every build testers can install is a public tag on `main`, and the app
  names its commit.
- Nothing reaches testers without the maintainer: agents stop at internal
  testers (ADR 0091).

## Consequences

- Open for an App Store submission grill, not decided here: the age rating,
  the storefront list (all but France, per above), the EU trader (DSA)
  declaration, screenshots and description, and the iPad device run.
- `scripts/verify-version.sh`'s iOS counterpart in rollout item 6 accepts
  `ios-v<x.y.z>-rc.N`.
- Order of work: this record and the docs (privacy, release run, P4); an
  iOS PR with the manifest, the encryption key, the core commit and the
  build number; #576 and item 3's device run; P17 and the manual archive of
  build 1 (`ios-v0.1.0-rc.2`); section 13 on that build and item 4; the Admin key and
  the hosted workflow; then the Lawyers group.

References: [interview](../design/testflight-grill-2026-11-03.md).
