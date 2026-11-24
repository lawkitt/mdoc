# mdoc for iOS

The iPhone and iPad app: scan or open a document, review the names mdoc
finds, share the pseudonymized text to an AI app and restore its reply.
SwiftUI on iOS 26 and later over the Rust core (`crates/mdoc-ffi`); no
third-party Swift packages.

```sh
just ios-app
just ios-test
just ios-test "iPhone 17" mdocUITests/LivePreviewUITests   # only these
```

They need the Xcode and Simulator runtime pinned in `scripts/tools.env`: on a
Mac, the builds pin's Xcode on the tests pin's runtime (ADR 0124).
Builds are unsigned, arm64 only. A TestFlight build is a tagged `main` commit
([iOS betas](../../docs/specs/release.md#ios-betas)) built from full history,
since the build number is the commit count.

`Info.plist` sets `ITSAppUsesNonExemptEncryption` to NO: the core's AES-GCM
is a standard algorithm, and off the App Store in France it needs no
documentation.

## On a phone

Signing is automatic, with the maintainer's team. With that Apple ID in Xcode, the phone connected, unlocked, trusted and in Developer
Mode:

```sh
just ios-core
xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -configuration Release \
  -destination id=<hardware UDID> -derivedDataPath target/ios/DeviceBuild \
  -allowProvisioningUpdates -allowProvisioningDeviceRegistration build
xcrun devicectl device install app --device <hardware UDID> \
  target/ios/DeviceBuild/Build/Products/Release-iphoneos/mdoc.app
```

The hardware UDID is the `00008150-…` id `xcodebuild` lists. The App Group
`group.io.github.lawkitt.mdoc` must exist in the team: open the project in
Xcode once and let Signing & Capabilities register it (the command line
cannot). A new developer certificate needs one tap of an installed app on the
phone with `ppq.apple.com` reachable before iOS lets it launch.

Device tests: [ui-tests.md](ui-tests.md).

## Project

Each folder is a synchronized group: adding a file needs no project edit.

The core finds ONNX Runtime through `OrtGetApiBase` in its own image, so
`MdocFFI` keeps that symbol, strips local symbols only and builds no debug
dylib. PDFium's prebuilt binary has no DWARF: PDFium frames in crash reports
get function names but no file or line.

Archiving in Xcode rewrites the `InfoPlist.xcstrings` catalogs, so they are
committed as Xcode writes them.

## UI changes

Check a change in the `#Preview`s (light, dark, AX5, iPad), then run the
snapshot tests. An intended change re-records them with
`TEST_RUNNER_MDOC_RECORD_SNAPSHOTS=1` and commits the PNGs; a component iOS
27 draws differently has its own `-ios27` reference (`Snapshot.swift`).

## Rules

Beside the [spec](../../docs/specs/mobile.md)'s UI and app-structure rules:

- **Edit mode types into the core.** Each committed edit, never marked IME
  text, goes to the session; the view is restyled in place, never given new
  text.
- **Long jobs** report a fraction, since iOS stops a continued task that
  shows no progress. While a job runs, review waits: a Replace would make
  the core throw the scan away.
- **Locked phone.** A result or save that comes while locked is saved on
  unlock, because Data Protection Complete blocks writes.
- **Storage.** Everything lives in the App Group container, excluded from
  backups; all but models use Data Protection Complete. The Vault key is in
  the Keychain's shared access group, this device only.
- **Downloads.** "Waiting for Wi-Fi" comes from `NWPathMonitor`, because
  background sessions never call `taskIsWaitingForConnectivity`.
- **Core calls** stay synchronous on the main actor while a review tap on
  the iPhone 12 Pro takes under about 16 ms.
- **Handlers the system calls on its own queues** carry no actor
  isolation, because a main-actor check there crashed every long job on a
  phone.
- **Drawers.** `SideDrawer`'s drag moves layers by transform and changes no
  SwiftUI state per frame, because that lagged. An `.inspector` never goes
  inside the drawer's hosting controller, because it reset its child's
  traits without end.
- **Menus.** A system menu draws every row's words in one color, so the
  row's icon carries the category color.
- **Candidate links** never show the system's link menu, because it exposed
  the raw `mdoc-candidate://` URL and Share.
- **No cross-fade** entering or leaving Adjust Words or Select, because both
  layouts showed at once.
- **Drag-merge** is the app's own gesture on the ≡ handle, because
  SwiftUI's List never delivers a drop of a drag that started inside it.
- **Table cells.** The text view drags a cell selection's handles itself,
  because UIKit's gesture hit-tests the table's hidden source.
- **Tips.** TipKit decides when a hint shows; mdoc draws it. UI tests hide
  tips unless `MDOC_SHOW_TIPS=1`. Reset tips resets each tip's eligibility,
  because `Tips.resetDatastore()` works only before TipKit starts.
