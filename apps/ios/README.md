# mdoc for iOS

The iPhone and iPad app ([ADR 0085](../../docs/adr/0085-mobile-apps.md)):
scan or open a document, review the names mdoc finds, share the
pseudonymized text to an AI app and restore its reply. SwiftUI on iOS 26
and later over the Rust core (`crates/mdoc-ffi`); no third-party Swift
packages.

```sh
just ios-app    # the core, then the app, extension and test targets for the Simulator
just ios-test   # unit, parity and UI tests on an iOS 26.1 Simulator
just ios-test "<Simulator name>"   # another device, such as an iPad
just mobile     # what local CI runs on the Mac, the iOS job included
just ios-device-test <hardware UDID>   # the device UI tests on a connected iPhone
```

They need Xcode with the iOS 26.1 platform. Both build the core first
(`just ios-core`: `target/ios/MdocCore.xcframework`, its Swift bindings and
`pdfium.xcframework`; a few seconds when nothing changed). Builds go to
`target/ios/DerivedData` and are unsigned. The core is arm64 only, so the
Simulator builds exclude x86_64.

## On a phone

The project signs with the maintainer's team (`DEVELOPMENT_TEAM =
F33DLM8K53`, automatic signing). With that Apple ID in Xcode, the phone
connected, unlocked, trusted and in Developer Mode:

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

### Device UI tests

The Simulator refuses `BGContinuedProcessingTask`, so only a phone runs a
long job through the real background task ([ADR 0088](../../docs/adr/0088-ios-on-real-devices.md)).
Before every TestFlight build after the first, with the phone connected,
unlocked, on Wi-Fi, and with UI Automation on (Settings → Developer):

```sh
just ios-device-test <hardware UDID>
just ios-device-test <hardware UDID> below-floor   # or detector: not by the phone's model
```

`scripts/ios-device-test.sh` builds for the phone, installs mdoc, copies the
fixtures into its data container with `devicectl` (the app ships none), runs
`ReviewFlowUITests` and the tier's test, and fails on a new
`mdoc-*.ips` crash report (or when it cannot list them). The fixtures are
the demo .docx, the 37-page `legal-opinion.docx`, a text PDF and six camera
pages it renders (`scripts/ios-scan-pages.swift`), in
`Library/mdoc-ui-fixtures` of the app's container: the UI-testing seed reads
a fixture there when its Mac path does not exist. The long-jobs test runs
at the detector tier with real downloads: the scan's Download & continue
fetches text recognition on Wi-Fi and recognizes the pages while the test
leaves the app for ten seconds, and Find names underlines the tenant. Find
names on the legal opinion takes over a minute; the test leaves for 45 s,
past the grace iOS gives without a continued processing task, and mdoc must
still be running (not suspended) and finish the job. The text PDF comes in
last. The first run also downloads the name detector, which later runs
keep; it skips on the Simulator.

The tier comes from the phone's model: the iPhone 15 Pro (`iPhone16,x`) and
every iPhone from the 16 on run `DeviceLongJobsUITests`; earlier phones run
`DeviceBelowFloorUITests`, which also runs on any phone with `below-floor`, as
UI tests take the tier from a stand-in memory size. Below the floor, Settings
says the phone cannot find names and Home has no setup card; the scan's
Download & continue fetches text recognition on Wi-Fi and recognizes the pages
while the test leaves for ten seconds; where Find names would be, the
document says so; a word selected and replaced as a person becomes
`PERSON_1`, and the text shares. Then the legal opinion comes in while the
test leaves for 15 s. Results go to `target/ios/device-test-*.xcresult`.

## Layout

The project is written by hand (`objectVersion` 77): each folder below is a
synchronized group, so adding a file needs no project edit.

| Folder | Target | What it holds |
| --- | --- | --- |
| `MdocFFI/` | `MdocFFI` framework | Compiles the generated bindings (`target/ios/swift/mdoc_ffi.swift`) over `MdocCore.xcframework`, plus `CoreFailure`, a public form of the core's error |
| `Shared/` | app and extension | `Core/`: the app's `Library`, `DocumentSession` and `Setup` protocols and value types, `RealCore` over the bindings, `FakeCore`, `CoreFactory`; `Storage/` (folders, the case key); the restore view and view model; `Localizable.xcstrings` |
| `mdoc/` | app | Home, the document screen, hand-off, setup and the background model downloader, Settings, the long-job runner |
| `RestoreExtension/` | Restore with mdoc | The share extension's entry point |
| `mdocTests/`, `mdocUITests/` | tests | View models against `FakeCore`; `RealCore`, the downloader and the Keychain store; parity with the desktop's goldens; one open → review → share → restore walk and the screenshots on the core |
| `Config/` | — | Info.plist additions and entitlements |

The app embeds `MdocFFI.framework` and `pdfium.framework`; the extension
links the app's copy of `MdocFFI`. The core reaches ONNX Runtime through
`OrtGetApiBase` in the image that holds it, so `MdocFFI` keeps that symbol
(`-u _OrtGetApiBase`), strips local symbols only and builds no debug dylib.

## Rules

- **Rust owns state.** View models keep the snapshots the core returns and
  view state only; every change is a core call, and a document saves into
  its case after each one. `CoreFactory` picks the core: `RealCore` in the
  app and the extension, `FakeCore` (fictional fixtures, in memory) only in
  the view-model tests.
- **Offsets are source bytes.** Mention ranges and Markdown spans are UTF-8
  offsets into the Markdown source; `TextOffsets` converts them for UIKit.
- **The core recognizes Markdown.** `MarkdownStyling` asks the core for a
  text's spans (`mdoc-markdown`, through `mdoc-ffi`'s `markdown_styling`):
  inline kinds, markers, line roles and the document's own colours.
  `MarkdownStyler` maps them to the app's fonts and colours, light and dark,
  with markers dimmed and candidates on top.
- **One long job at a time**, with progress and cancel. iOS 26 keeps it
  running in the background through `BGContinuedProcessingTask`; earlier
  versions get the short grace, then cancel and offer to resume.
- **OCR keeps finished pages.** Recognition imports through
  `import_file_in`, which keeps each recognized page in the core's
  `imports/` folder; after a cancel, or mdoc stopping, Resume recognition
  reads only the pages left. Dropping an import discards its pages
  (`discard_import`), and deleting a saved document goes through the core
  (`delete_document`), so it also leaves its case.
- **Storage.** Everything lives in the App Group container
  (`group.io.github.lawkitt.mdoc`), excluded from backups: the core's
  `documents/` and `cases/`, the source files in `originals/`, models and
  their downloads, and the core's kept OCR pages (`imports/`). All but
  models use Data Protection Complete. The case key
  is 32 random bytes in the Keychain (the shared access group, this device
  only, readable while unlocked).
- **Intake.** PDF, DOCX and pasted text go to the core as they are; camera
  and photo pages become upright JPEGs, and several pages one image-only
  PDF, which the core reads with OCR.
- **Models.** `Setup` offers what the device's RAM allows; the files
  download in a background `URLSession` (Wi-Fi unless the user allows
  cellular), and the core checks and installs each one. On any cellular
  interface mdoc asks first, whatever iOS's expensive flag says; a download
  held for Wi-Fi shows "Waiting for Wi-Fi" with Use cellular. That state
  comes from `NWPathMonitor`: background sessions never call
  `taskIsWaitingForConnectivity` ([ADR 0088](../../docs/adr/0088-ios-on-real-devices.md)).
- **Below the detector floor** there is no Find names and no detector setup
  card: where Find names would be, the document says the device cannot find
  names and that selecting text marks one; Settings says the same. Text
  recognition is offered in the document that needs it and in Settings.
- **The extension loads no model.** It restores with the library's current
  case or one the user picks.
- **Strings** live in `Shared/Localizable.xcstrings` (Info.plist strings in
  each target's `InfoPlist.xcstrings`); every visible string goes there.

UI tests launch the app with `-mdoc-ui-testing`: a fresh library of its own
in the App Group (the extension follows it), its own case key, and a device
below the detector floor, so no model is needed. The app imports the file in
`MDOC_SEED_FILE` (the demo DOCX, the Markdown-rich
`tests/fixtures/mobile-markdown/review.md` as pasted text, or a folder of
JPEG pages as a scan) and scans it at launch with the structured rules alone:
the only such scan, which gives the review candidates without a model. On a
phone, where the Mac path does not exist, the seed reads the file of the same
name that `just ios-device-test` copied into the app's container. Each launch
clears the test library but keeps a downloaded name detector. Tests that need
the OCR model download it and run only with `MDOC_OCR_TESTS=1` (the UI test
also sets `MDOC_REAL_DOWNLOADS=1` for the app); the device long-jobs test sets
it and `MDOC_UI_DETECTOR_TIER=1`.
