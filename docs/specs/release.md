# Releases

How desktop and mobile builds are cut, signed, accepted and published; the
recipes and workflows say how each step runs. Orchestrating a run across
machines is the `fleet` skill's job.

## Targets and channels

- Desktop ships for Apple Silicon macOS (`.dmg`) and Windows x64 (the NSIS
  `.exe`, no `.msi`, no `.zip`), through GitHub Releases only. Linux, Intel
  macOS and Windows arm64 build from source, unsupported. winget, Homebrew
  and Scoop return with signed `1.0` builds. No Nix.
- A future `.msi` reuses UpgradeCode `3D50EAE8-D339-41B6-82BC-126389601487`.
- The installer lists mdoc under Open with, never as the default app.
- iOS ships through TestFlight and then the App Store. Android ships to
  Play's internal track and as a Play-signed APK on a draft GitHub Release.

## Pipeline rules

- Every action is pinned to a full commit SHA; only the release-publishing
  job has `contents: write`. The default `GITHUB_TOKEN` is read-only,
  Actions cannot approve PRs, and fork PRs need approval to run.
- Each platform's secrets live in its own environment (`release`,
  `release-ios`, `release-android`), which admits only `main` and its own
  tags (`v*`, `ios-v*`, `android-v*`), with no required reviewers.
- No signing secret or upload key is on the maintainer's machines,
  readable from a feature branch, or in chat, files or remote URLs. Only the
  maintainer enters or revokes one.
- Nothing a user can install is published without the maintainer: agents
  stop at drafts and internal tracks.
- Never pipe answers to `just release`'s prompts; an unattended run passes
  `--yes`.

## macOS signing

To set up Developer ID signing or replace the certificate, the account
holder `cd`s alone into the directory whose direnv `.envrc` loads the
`CERTIFICATE_OSX_*` variables from the Keychain and runs this, which pipes
every value:

```sh
gh api -X PUT repos/lawkitt/mdoc/environments/release
for pair in APPLE_CERTIFICATE=CERTIFICATE_OSX_P12_DATA APPLE_CERTIFICATE_PASSWORD=CERTIFICATE_OSX_P12_PASSWORD APPLE_ID=CERTIFICATE_OSX_ID APPLE_PASSWORD=CERTIFICATE_OSX_APP_PASSWORD; do
  src=${pair#*=}
  [[ -n ${(P)src} ]] || { echo "$src is empty"; break; }   # zsh
  printf %s "${(P)src}" | gh secret set "${pair%%=*}" --env release -R lawkitt/mdoc
done
gh variable set APPLE_TEAM_ID --env release -R lawkitt/mdoc --body F33DLM8K53
gh variable set APPLE_SIGNING_IDENTITY --env release -R lawkitt/mdoc --body "Developer ID Application: Tebriz Tagiev (F33DLM8K53)"
```

- Signing exists because from macOS 15 right-click → Open no longer
  bypasses Gatekeeper, and lawyers give up at Open Anyway.
- The certificate expires on 2027-02-01 and the membership renews yearly:
  replace the secrets before expiry. A lapse blocks new builds, never
  published ones.
- Windows builds stay unsigned until `1.0`, because Microsoft's Trusted
  Signing limits individual eligibility and SmartScreen's "Run anyway" is
  one click.
- A Developer ID provisioning profile (App ID `io.github.lawkitt.mdoc` with
  **Keychain Sharing**, in `APPLE_PROVISIONING_PROFILE_BASE64`) keeps the
  Vault key in the data protection keychain, so macOS never asks for the
  login password.
- Before publishing a release that changes Vault key handling, run
  `scripts/desktop/keychain-launch-check.sh` on a clean macOS account.

## Release candidates

- A minor gets one RC and full desktop acceptance; must-fix fixes are
  re-checked on a local unsigned `just package` build, only the rows each
  touches.
- Cut another RC only for a feature after rc.1, or a fix to what only a
  signed, installed build exercises (installer, signing, update, the
  keychain case key); full acceptance then runs on it.
- A patch gets no RC: a smoke run on both platforms (install, launch, open a
  document, Copy for AI) plus the rows its fixes touch.
- A release publishes once both platforms' runs end with no must-fix Fail.
- Desktop acceptance keeps at most 10 rows and iOS at most 8, so a run fits
  one sitting.
- During acceptance only test-only and CI-only PRs merge; product PRs and
  fork rev bumps wait for both runs.

## Desktop acceptance

Once per minor, on the RC's draft artifacts, on the Mac and the PC. Headless
tests, release gates and the hosted Windows install check cannot establish
these.

| ID | Check |
|---|---|
| D1 | *macOS:* the `.dmg` installs; `spctl -a -vv /Applications/mdoc.app` reports Notarized Developer ID, and launch shows no Gatekeeper warning beyond "downloaded from the Internet" |
| D2 | *Windows:* SmartScreen's "Run anyway" proceeds, the installer completes with the VC++ runtime present, mdoc launches from Start, and the uninstaller removes it |
| D3 | A clean first launch shows Home with no error and no Keychain, network or crash-report prompt, and About shows the expected version |
| D4 | *macOS:* adding the first document shows the Keychain explainer, then the system dialog; after quit and relaunch, a Project's document restores its replacements with no prompt |
| D5 | *Windows:* adding a document asks nothing (DPAPI); after quit and relaunch, a Project's document restores its replacements |
| D6 | Model setup: offline, the consent card's download fails cleanly with Retry; online, it downloads with progress, verifies and resumes the scan |
| D7 | Russian typed into the editor and the find field appears correctly, and a misspelled Russian word gets the spelling underline |
| D8 | Dropping files from Finder or Explorer onto Home and the sidebar opens them, and "Open with" from the OS opens a file in mdoc |
| D9 | *macOS:* the menu bar shelf shows its documents; Copy for AI and dragging a ready document into a browser composer both deliver the aliased text |
| D10 | *macOS:* quitting with unsaved work asks first, from ⌘Q, the Dock and logout |

- Start from a clean profile: quit mdoc and rename its app-data folder,
  then restore it. Never delete the Vault key (`io.github.lawkitt.mdoc` /
  `case-key`, or `cases/key.dpapi`, names from before the Vault): the Vault
  becomes unreadable. D4 needs a
  macOS user that never ran mdoc.
- Show only public fixtures (`docs/product-media/`, `tests/fixtures/`);
  copy one into `Документы/Папка с пробелами/` for the path rows.
- Results go in one issue per release, each row Pass, Fail or Unavailable.
  A Fail gets its own issue and the run goes on; nothing is fixed during a
  run.
- A Fail is must-fix when it loses data, blocks a listed flow, or leaves a
  control unreachable by keyboard; anything else becomes a roadmap item.

## iOS betas

- Agents never sign in to App Store Connect, and only the maintainer
  changes `MARKETING_VERSION`.
- **Gate:** `just ios-device-test` on the commit to be tagged. If the gate
  phone is unreachable, tell the maintainer and tag nothing; the gate is
  never skipped. A build for Lawyers also passes on the iPhone 17 Pro,
  which gets mdoc from TestFlight only.
- **What to Test**, kept outside the repository, says what is new for
  testers since the last uploaded build (from the `changes/ios/` and
  `changes/mobile/` fragments; recognition in a line or none), what to try
  and the known gaps.
- An agent on the Mac with the gate's phone tags with `just release-ios`
  and reports version, build and run link once the build is in
  "Maintainer". Rerun a runner or network failure once; any other failure
  gets a fix PR and a new `rc.N`.
- Builds for the "Lawyers" group (5 to 10 invited testers, no public link)
  go at most weekly, for a fix that affects testers; internal builds have
  no cap. The maintainer adds testers, answers Beta App Review and releases
  the plain `ios-v<x.y.z>`.
- The App Store Connect key has the Admin role, the least cloud signing
  accepts.
- Testers send feedback through TestFlight only. App Store submission waits
  for a run on a real iPad.
- **Last resort**, hosted macOS down for long: the maintainer archives a
  clean full-history checkout of the tag in Xcode (after
  `MDOC_REQUIRE_FULL_HISTORY=1 just ios-core`) and distributes with "App
  Store Connect", never "TestFlight Internal Only".

## iOS acceptance

On a TestFlight build, by the maintainer, before Lawyers get a version.

| ID | Check |
|---|---|
| I1 | Scan with the document camera recognizes a printed English and Russian page |
| I2 | On cellular, the model download asks first; leaving the app keeps it going |
| I3 | After locking the phone, the Vault is unreadable to a backup or file browser, and a restored backup starts empty |
| I4 | A 20-page scan and Find names keep running in the background, or offer to resume on return |
| I5 | Share to the ChatGPT or Claude app sends aliases only; Restore with mdoc from the AI app restores the names |
| I6 | Below the device floor, setup offers only text recognition and the app says why it cannot find names |

## Android

- Google holds the app signing key (Play App Signing); CI holds only a
  resettable upload key in `release-android`. There is no local release
  path.
- Run `just android-device-test` before every Android release after the
  first.
- Play's Data safety form says no data is collected.
- Firebase Test Lab phones get public fixtures only; Test Lab closes on
  2027-09-30.
- Closed testing follows iOS: `android-v<x.y.z>-rc.N` tags, one invite-only
  "Lawyers" track, the same "never client files" line.
