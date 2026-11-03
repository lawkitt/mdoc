# TestFlight grill, 2026-11-03

Interview with the maintainer on publishing the iOS app
([ADR 0085](../adr/0085-mobile-apps.md)) to TestFlight, ROADMAP iOS 0.1
item 1. Decisions are recorded per round; the ADR follows when the tree
closes.

## Facts found before round 1

- **Account.** Team `F33DLM8K53` is a paid Apple Developer Program
  membership in the maintainer's own name (one-year team profiles for
  `io.github.lawkitt.mdoc` and `io.github.lawkitt.mdoc.RestoreExtension`,
  created 2026-10-31). This Mac holds an Apple Development identity only; no
  Apple Distribution certificate.
- **Project.** Automatic signing, `MARKETING_VERSION = 0.1.0`,
  `CURRENT_PROJECT_VERSION = 1`, iOS 26.0 minimum, iPhone and iPad, no Mac
  Catalyst or "Designed for iPad". Entitlements: App Group
  `group.io.github.lawkitt.mdoc`, Keychain group
  `$(AppIdentifierPrefix)io.github.lawkitt.mdoc.shared`, and (app only)
  `com.apple.developer.kernel.increased-memory-limit`. An app icon exists.
- **Missing for an upload.** No `PrivacyInfo.xcprivacy` (the app reads
  `UserDefaults`, its own and the App Group suite's), no
  `ITSAppUsesNonExemptEncryption` key, no core commit in Info.plist or
  Settings (ADR 0091 requires one), no `just release-ios` recipe, no iOS
  step in `docs/release-run.md`, no iOS section in `docs/privacy.md`.
- **Encryption.** Cases are AES-256-GCM through the RustCrypto `aes-gcm`
  crate (ADR 0041), not Apple's CryptoKit; downloads use HTTPS through
  `URLSession`.
- **Versions.** `main` has 718 commits (the ADR 0091 build number today).
  Desktop pre-releases are `v<x.y.z>-rc.N` tags; `CFBundleShortVersionString`
  cannot hold `-rc.N`.
- **Licence.** The repository is public; Settings › About shows the GPL and
  app-store permission text and links the repository root, not the build's
  commit.
- **Acceptance.** Section 13 has P1–P18. P4 still describes the withdrawn
  rules-only tier (ADR 0088 replaced it). ROADMAP iOS items 3 (device UI
  tests on Wi-Fi) and 4 (below the floor on the iPhone 12 Pro) are open.
- **Name.** "MDOC Wellness" (Michigan Department of Corrections) is on the
  App Store; whether the plain name "mdoc" is free shows only when the app
  record is created.

## Round 1

All eight accepted as recommended.

1. **Seller: the individual membership.** TestFlight and the App Store show
   the maintainer's name. An organization (D-U-N-S, legal entity) is
   revisited only if one exists; apps can move between accounts later.
2. **The app record exists**, created by the maintainer in App Store
   Connect before any upload (Apple ID 6830176855): bundle ID
   `io.github.lawkitt.mdoc`, SKU `mdoc-ios`, English (U.S.), not offered
   on Apple silicon Macs or Apple Vision Pro. Agents never sign in to App
   Store Connect.
3. **Build 1 by hand.** An agent prepares a clean checkout of a `main`
   commit carrying the round-1 changes; the maintainer archives it in Xcode
   on the macbook-air, signed in with their Apple ID, and distributes with
   "App Store Connect" (not "TestFlight Internal Only", which can never go
   to external testers). Apple-managed signing creates the distribution
   certificate in the cloud; no certificate or key lands in the repository,
   chat or files.
4. **Beta tags are `ios-v<x.y.z>-rc.N`.** `MARKETING_VERSION` stays
   `x.y.z` through the beta, the build number (commits on `main`) grows,
   and the plain `ios-v<x.y.z>` tag marks the build submitted to App Store
   review. Build 1 is `ios-v0.1.0-rc.1`. Each uploaded build is a public tag,
   which is how its Corresponding Source stays available (ADR 0086). Amends
   ADR 0091's tag rule.
5. **Export compliance: standard algorithms, not in France for 0.1.** Cases
   use AES-256-GCM outside Apple's OS, so the answer is "standard
   encryption algorithms in addition to Apple's"; the App Store's France
   storefront stays off until a French (ANSSI) declaration is filed.
   `ITSAppUsesNonExemptEncryption = YES` goes in Info.plist; the maintainer
   checks the form's wording at the first upload. Moving iOS case
   encryption to CryptoKit was rejected: the core owns it (ADR 0085).
6. **A pre-upload iOS PR** adds `PrivacyInfo.xcprivacy` to the app and the
   extension (`UserDefaults` reasons CA92.1 and 1C8F.1, file timestamps
   C617.1 if the core's `stat` calls need it, no tracking), the encryption
   key, the core commit as an Info.plist key shown in Settings › About with
   the source link pointing at that commit, the build number from `main`'s
   commit count at build time, and P4 rewritten for ADR 0088. `just
   release-ios` and the hosted workflow stay ADR 0091 rollout item 6.
7. **Build 1's gate** (internal: the maintainer only): #576 merged, ROADMAP
   iOS item 3's full device run passing on the iPhone 17 Pro, and P17 by
   hand on the 17 Pro on a development build of the same commit. Item 4
   (below the floor, iPhone 12 Pro) gates external testers, not build 1.
   All of section 13 then runs on the TestFlight build itself.
8. **Privacy: "Data Not Collected"**, and `docs/privacy.md` gains an iOS
   section (model downloads only, pinned SHA-256, no update check, crash
   reports stay on the phone, nothing in backups) whose GitHub URL is the
   privacy policy URL. Crash reports and feedback testers send through
   TestFlight are Apple's channel under the tester's agreement.

## Round 2

All nine accepted as recommended.

9. **The store name is "mdoc: private docs for AI"** (the record took the
   fallback). The home-screen name stays `mdoc` (`CFBundleDisplayName`).
10. **One external group, "Lawyers"**, 5 to 10 people the maintainer
    invites by email; no public link while names can still be missed. Each
    joins only after ROADMAP iOS item 4 and section 13 pass on a TestFlight
    build. The first external build of each version goes through Beta App
    Review.
11. **Beta App Review notes** say: no account or sign-in; the optional
    downloads are model data from pinned URLs checked by SHA-256, not code
    (guideline 2.5.2); phones under 8 GB cannot find names by design and
    mark names by selection; steps: + → Paste a short fictional paragraph
    (in the notes) → Find names → Share → Restore with mdoc. An agent drafts
    them in the repository; the maintainer pastes them into App Store
    Connect.
12. **Feedback through TestFlight only**, screenshot feedback and the crash
    reports testers choose to share; testers are not asked to file GitHub
    issues. The beta description and every What to Test open with: "Test
    with documents you are allowed to share, never client files; check a
    screenshot before you send it." The feedback email shown to testers is
    the maintainer's choice.
13. **What to Test.** `rc.1`'s is written by hand: the flow to try, the
    known gaps (below-floor phones, English UI only, no iPad device check
    yet) and the line above. `changes/ios/` joins `apps/ios/CHANGELOG.md`
    at `ios-v0.1.0-rc.1`; from `rc.2` the version's CHANGELOG section fills
    What to Test (ADR 0091).
14. **iPad stays.** P12 runs in the iPad Simulator for TestFlight, marked
    "Simulator only" in the evidence; App Store submission waits for an
    iPad device run (the maintainer's or an external tester's).
15. **New builds** only when a fix affects testers, at most weekly, each
    after `just ios-device-test` on both phones (detector tier on the 17
    Pro, below-floor on the 12 Pro). Build 2 on comes from the hosted
    workflow (ADR 0091 item 6); build 1 is the only manual upload.
16. **The workflow's App Store Connect key** is a Team key with the Admin
    role (the least that cloud-managed signing accepts, per Apple's
    documentation; confirmed at setup). The maintainer puts its issuer ID,
    key ID and `.p8` straight into the `release-ios` environment's secrets;
    they never pass through chat, files or an agent. A lesser role replaces
    it if Apple ever accepts one.
17. **`docs/release-run.md` gains "iOS beta build"**: the gate (round 1
    item 7, or item 15 for later builds), the `rc.N` tag (and build 1's
    manual archive), the export-compliance answer, What to Test, internal
    install and P1–P18 into `docs/evidence/mobile/<date>/ios.md`, then the
    maintainer adds the external group.

## Round 3

All three accepted as recommended; the maintainer confirmed the shared
understanding.

18. **Android follows the same pattern** at Play's closed testing:
    `android-v<x.y.z>-rc.N` tags (`versionName` fixed, `versionCode` from
    the commit count), one invite-only "Lawyers" track, the same "never
    client files" line, agents stopping at the internal track. Nothing is
    built now.
19. **Left for an App Store submission grill:** the age rating, the
    storefront list (all but France), the EU trader (DSA) declaration
    (trader shows the maintainer's address and phone in the EU), screenshots
    and description, and the iPad device run.
20. **Order of work:** (1) this docs PR: the record, [ADR
    0097](../adr/0097-ios-testflight-beta.md), glossary terms, P4,
    `docs/privacy.md`'s iOS section, `docs/release-run.md`'s iOS beta
    section, ROADMAP item 1; (2) the iOS pre-upload PR (manifest,
    encryption key, core commit, build number); (3) #576 and ROADMAP iOS
    item 3's device run; (4) P17 on the iPhone 17 Pro, then the manual
    archive of `ios-v0.1.0-rc.1`; (5) section 13 on that build and item 4 on
    the iPhone 12 Pro; (6) the Admin key in `release-ios` and ADR 0091 item
    6's workflow; (7) the Lawyers group.
