# Releases

Folds: 0037, 0057, 0083, 0091, 0097, 0107.

How desktop and mobile builds are cut, signed, accepted and published.
Orchestrating a release run across machines (hub, lanes, GUI sessions,
polling) is the `fleet` skill's job, not this repository's.

## Targets and channels

- Desktop ships for Apple Silicon macOS (`.dmg`) and Windows x64 (the NSIS
  `.exe`, no `.msi`, no `.zip`). Linux, Intel macOS and Windows arm64 build
  from source, unsupported; `lint (ubuntu)` keeps Linux compiling, with no
  runtime checks.
- Desktop ships through GitHub Releases only. winget, Homebrew and Scoop
  return with signed `1.0` builds. No Nix.
- A future `.msi` reuses UpgradeCode `3D50EAE8-D339-41B6-82BC-126389601487`.
- The Windows installer bundles `vc_redist.x64.exe` and runs it, elevated,
  only when the runtime is missing; CI checks its Authenticode signature.
- The installer lists mdoc under Open with for `.md`, `.markdown`, `.mdown`,
  `.txt`, `.pdf` and `.docx`, never as the default app, and the uninstaller
  removes exactly those entries. A second launch hands its files to the
  running mdoc through a per-user named pipe and exits.
- iOS ships through TestFlight and then the App Store. Android ships to
  Play's internal track and as a Play-signed APK on a draft GitHub Release.

## Pipeline rules

- Every action is pinned to a full commit SHA; only the release-publishing
  job has `contents: write`. The default `GITHUB_TOKEN` is read-only,
  Actions cannot approve PRs, and fork PRs need approval to run.
- Local recipes and CI run the same commands, with tool versions from
  `scripts/tools.env`; everything runs natively on the Apple Silicon Mac and
  the Windows x64 PC.
- Release gates test the shipped artifact: `just smoke-package` runs the
  packaged app's OCR self-test on the English and Russian fixtures, on macOS
  from the `.dmg` under its hardened runtime with
  `disable-library-validation`, because only a packaged build shows a
  signature refusing the downloaded runtimes.
- Heavy and machine-dependent tests (performance, installed models, local
  files) stay `#[ignore]` and run only through recipes.
- Each platform's secrets live in its own environment: `release`,
  `release-ios`, `release-android`. Each admits only `main` and its own tags
  (`v*`, `ios-v*`, `android-v*`), with no required reviewers. No platform's
  workflow can read another's keys.
- No signing secret or upload key is on the maintainer's machines,
  readable from a feature branch, or in chat, files or remote URLs. Only the
  maintainer enters or revokes one.
- Nothing a user can install is published without the maintainer: agents
  stop at drafts and internal tracks.

## Desktop release

- The repository version is the source of truth. `just release <version>`
  checks that `main` is clean and current, runs the checks (`--check-all`
  adds the smoke tests), sets the version in `Cargo.toml`, `Cargo.lock` and
  `resources/Info.plist`, joins the top-level `changes/*.md` fragments into
  CHANGELOG's Unreleased section, names that section for a
  stable version, commits `release: v…`, tags, and pushes after
  confirmation.
- In a run with no one to answer: `just release <version> --yes
  --skip-checks-if-ci-green`. Never answer its prompts by piping input.
  `--no-push` wins over `--yes`. The skip applies only when the latest
  CI-relevant `main` commit has a green CI push run (or, for a
  `release: v…` commit, a green Release run on its tag); anything uncertain
  runs the local checks.
- `-beta.N` and `-rc.N` tags are pre-releases. The tag must match
  `Cargo.toml`, and highlights come only from a matching `## [x.y.z]`
  section.
- The tag runs `release.yml`: the version check, CI with the OCR smoke test
  on both targets, packaging, the packaged-app gate, then a **draft**
  GitHub Release with `SHA256SUMS` and build provenance. The tag run skips
  the called CI when the release commit changes only version lines,
  `CHANGELOG.md` and `changes/` and its parent has a green `ci.yml` push run
  (waiting up to about 20 minutes for one still running).
- A hosted Windows job installs the NSIS artifact silently, launches it on
  a fresh profile, checks the main window through UI Automation, quits and
  uninstalls; an impossible-title case proves that startup failures fail.
  It establishes startup only, not acceptance.
- Check that the `.dmg` and the `.exe` are on the draft. Only the
  maintainer publishes.
- "Run workflow" on Release builds an unpublished test build from any
  pushed branch: straight to packaging unless **checks** is ticked, signed
  only with **sign**, which works from `main` only.
- `just smoke-update [dmg]` checks that an older signed `.dmg` updates
  itself; run it before a release that changes the updater.

## macOS signing

Signed builds are Developer ID signed and notarized from the `release`
environment. To set it up, or to replace an expiring
certificate, the account holder opens a shell in a directory whose direnv
`.envrc` loads the `CERTIFICATE_OSX_*` variables from the Keychain (`cd`
there on its own, so direnv runs first), then runs the commands below.
Values are piped, never on a command line, and an empty variable stops the
loop.

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

Then run the Release workflow with **sign** ticked. To sign locally, import
the certificate into the login keychain, run
`xcrun notarytool store-credentials mdoc-notary` once, then
`APPLE_SIGNING_IDENTITY="Developer ID Application: Tebriz Tagiev (F33DLM8K53)" APPLE_KEYCHAIN_PROFILE=mdoc-notary just package`.

Windows builds stay unsigned until `1.0`; SmartScreen's "Run anyway" is one
click.

## Case key

- With a Developer ID provisioning profile the case key lives in the data
  protection keychain, and macOS never asks for the login password;
  without one, signed builds use the login keychain. To turn it
  on, the account holder registers the App ID `io.github.lawkitt.mdoc` with
  **Keychain Sharing**, makes a **Developer ID** provisioning profile and
  stores it:
  `base64 -i mdoc.provisionprofile | gh secret set APPLE_PROVISIONING_PROFILE_BASE64 --env release -R lawkitt/mdoc`.
  Locally, set `APPLE_PROVISIONING_PROFILE=<path>` beside the signing
  variables.
- Unsigned, ad-hoc and debug builds use their own login-keychain key
  (`io.github.lawkitt.mdoc.dev`) and `mdoc/cases-dev/`, never release cases.
- Before publishing a release that changes case-key handling, install it on
  a clean macOS user account and run
  `scripts/desktop/keychain-launch-check.sh`: launch must show no Keychain
  dialog and leave cases locked.

## Release candidates

- A minor gets one RC and the full desktop acceptance below. Must-fix fixes
  then merge to `main` and are re-checked on a local unsigned
  `just package` build, only the rows each touches; the stable is tagged and
  gets a smoke run.
- Cut another RC only when a must-fix fix touches what only a signed,
  installed build exercises: the installer, signing or notarization, the
  update check or in-app update, or the case key in the keychain. A feature
  added after rc.1 also makes a new RC; full acceptance then runs on the
  last RC and earlier ones get an install and smoke check.
- A patch gets no RC: a smoke run on both platforms (install, launch, open a
  document, Copy for AI) plus the rows its fixes touch.
- A patch or stable publishes once both platforms' runs are done with no
  must-fix Fail; rows only the maintainer can do may follow, listed in the
  report.
- Before an RC, `ROADMAP.md`'s Current status is up to date.
- During acceptance, test-only and CI-only PRs merge when green; product
  PRs (app code, forks) wait, and no fork rev is bumped, until both runs
  finish.

## Desktop acceptance

Run once per minor release on the RC's draft artifacts, on the Apple
Silicon Mac and the Windows x64 PC. Headless tests and the release gates
cannot establish these rows.

| ID | Check |
|---|---|
| D1 | *macOS:* the `.dmg` installs; `spctl -a -vv /Applications/mdoc.app` reports Notarized Developer ID, and launch shows no Gatekeeper warning beyond "downloaded from the Internet" |
| D2 | *Windows:* SmartScreen's "Run anyway" proceeds, the installer completes with the VC++ runtime present, mdoc launches from Start, and the uninstaller removes it |
| D3 | A clean first launch shows Home with no error and no Keychain, network or crash-report prompt, and About shows the expected version |
| D4 | *macOS:* Save as Case shows the Keychain explainer, then the system dialog; after quit and relaunch, a case member restores its replacements with no prompt |
| D5 | *Windows:* saving a case asks nothing (DPAPI); after quit and relaunch, a case member restores its replacements |
| D6 | Model setup: offline, the consent card's download fails cleanly with Retry; online, it downloads with progress, verifies and resumes the scan |
| D7 | Russian typed into the editor and the find field appears correctly, and a misspelled Russian word gets the spelling underline |
| D8 | Dropping files from Finder or Explorer onto Home and the sidebar opens them, and "Open with" from the OS opens a file in mdoc |
| D9 | *macOS:* the menu bar shelf shows its documents; Copy for AI and dragging a ready document into a browser composer both deliver the aliased text |
| D10 | *macOS:* quitting with unsaved work asks first, from ⌘Q, the Dock and logout |

- Install from the draft and start from a clean profile: quit mdoc and
  rename its app-data folder (`~/Library/Application Support/mdoc`,
  `%LOCALAPPDATA%\mdoc`), then restore it afterwards. Never delete the case
  key (`io.github.lawkitt.mdoc` / `case-key`, or `key.dpapi` inside the
  Windows folder): saved cases become unreadable. D4's first dialog needs a
  macOS user that never ran mdoc.
- Only public fixtures on screen and in notes: the demos in
  `docs/product-media/`, `tests/fixtures/docx-preview/legal-opinion.docx`
  and `tests/fixtures/reference.pdf`. Copy a demo into
  `Документы/Папка с пробелами/` for the path rows.
- Rows that need a physical keyboard, a password, system settings or a
  drag from Finder or Explorer are the maintainer's.
- Record results in one GitHub issue per release: build, machines and OS
  builds, then each row Pass, Fail or Unavailable with a note. A Fail gets
  its own issue in the release's milestone, linked from the row, and the
  run goes on; nothing is fixed during a run.
- A Fail is must-fix when it loses data, blocks a listed flow, or leaves a
  control unreachable by keyboard; anything else becomes a roadmap item.

## iOS betas

- The App Store record belongs to the maintainer's individual membership
  (team `F33DLM8K53`): name "mdoc: private docs for AI", bundle ID
  `io.github.lawkitt.mdoc`, SKU `mdoc-ios`, not offered on Apple silicon
  Macs or Vision Pro. Agents never sign in to App Store Connect.
- A beta is an annotated `ios-v<x.y.z>-rc.N` tag on a tested `main` commit,
  with no release commit. `MARKETING_VERSION` stays `x.y.z` and only the
  maintainer changes it; the build number is the commit count on `main` up
  to the tagged commit. Tags are never moved or reused.
- **Gate:** `just ios-device-test <UDID>` on the iPhone 12 Pro, the slowest
  phone that finds names, at its own tier, on the commit to be tagged, from
  a clean checkout. If the phone is unreachable, tell the maintainer and tag
  nothing; the gate is never skipped, and a forced tier or a newer phone
  never counts. A build for Lawyers also passes the detector tier on the
  iPhone 17 Pro.
- The iPhone 17 Pro gets mdoc from TestFlight only: install nothing on it
  unless the maintainer asks.
- **What to Test**, in a file outside the repository: first "Test with
  documents you are allowed to share, never client files; check a
  screenshot before you send it.", then what is new since the previous
  uploaded build (from the `changes/ios/` fragments, for testers; a
  recognition change in a line or none), what to try and the known gaps,
  under about 2,000 characters.
- **Tag:** `just release-ios <x.y.z>-rc.N <file>`, by an agent on the Mac
  with the gate's phone. The tag's `ios-release.yml` run tests, archives,
  signs with Apple-managed signing, uploads, waits for processing and sets
  What to Test. Report version, build and run link once the build is in
  "Maintainer". Rerun a runner or network failure once; any other failure
  gets a fix PR and a new `rc.N`. An upload Apple already has counts as
  done. `ios-release.yml` has a validate mode
  (from `main`, no upload) and a finish mode (a tag that uploaded and then
  failed).
- Internal builds have no cap. Builds for the "Lawyers" group (5 to 10
  invited testers, no public link) go at most weekly, for a fix that
  affects testers. The maintainer adds testers, answers Beta App Review and
  releases the plain `ios-v<x.y.z>` for App Store review, when a PR joins
  the `changes/ios/` fragments into `apps/ios/CHANGELOG.md`.
- Signing uses an App Store Connect Team key with the Admin role, the
  least cloud signing accepts, kept only in `release-ios` (`ASC_KEY_ID`,
  `ASC_ISSUER_ID`, `ASC_KEY_P8`); no certificate lives on the runner.
- Testers send feedback through TestFlight only. App Store submission waits
  for a run on a real iPad.
- Every build carries `ITSAppUsesNonExemptEncryption` NO (cases use a
  standard algorithm besides Apple's), and the App Store in France stays
  off; offering France needs YES and a French declaration.
- The app and the Restore extension carry a privacy manifest (no tracking,
  no collected data), the privacy label is "Data Not Collected" with
  `docs/privacy.md`'s iOS section as the policy URL, and
  Settings › About names the build's core commit and links its source.
- **Last resort**, hosted macOS unusable for long: the maintainer archives
  a clean full-history checkout of the tag in Xcode
  (`MDOC_REQUIRE_FULL_HISTORY=1 just ios-core` first) and distributes with
  "App Store Connect", never "TestFlight Internal Only".
- The beta description and Beta App Review notes are in App Store Connect.
  The notes say: no account; the only downloads are model data from pinned
  URLs checked by SHA-256; phones under 8 GB cannot find names by design;
  and a fictional paragraph to paste, Find names, Share and Restore with
  mdoc.

## iOS acceptance

On a TestFlight build, by the maintainer, before Lawyers get a version.
Results go in the release's issue.

| ID | Check |
|---|---|
| I1 | Scan with the document camera recognizes a printed English and Russian page |
| I2 | On cellular, the model download asks first; leaving the app keeps it going |
| I3 | After locking the phone, documents and cases are unreadable to a backup or file browser, and a restored backup starts empty |
| I4 | A 20-page scan and Find names keep running in the background, or offer to resume on return |
| I5 | Share to the ChatGPT or Claude app sends aliases only; Restore with mdoc from the AI app restores the names |
| I6 | Below the device floor, setup offers only text recognition and the app says why it cannot find names |

## Android

- `android-v<x.y.z>` tags release from `main`. Google holds the app signing
  key (Play App Signing); CI holds only a resettable upload key in
  `release-android`. The workflow checks that the tag is on `main` and
  matches the repository's version, runs the hosted Android job and the
  minified release-like tests, then uploads the AAB to Play's internal track
  and attaches the APK Play signs to a draft GitHub Release. There is no
  local release path.
- Run `just android-device-test` before every Android release after the
  first.
- Closed testing follows iOS: `android-v<x.y.z>-rc.N` tags, one invite-only
  "Lawyers" track, the same "never client files" line.

## Mobile builds

- The build number is the commit count on `main` up to the tagged commit,
  and each app records its core commit and shows it in Settings and crash
  reports.
- Each app has its own changelog fed by `changes/ios/` or
  `changes/android/`; `just release` leaves those. A core change users see
  on more than one platform gets a fragment in each folder.
- Release configurations are tested before upload: the iOS Release build
  in the Simulator, and the minified Android build on the emulator.
- Dependabot and fork PRs build only on hosted runners.
- Gradle verification metadata is regenerated only by a manually dispatched
  hosted workflow with `contents: read` that uploads the diff as an
  artifact, never locally and never through `pull_request_target`. An agent
  checks that only the bumped packages and their checksums changed before
  committing it to the Dependabot branch.
