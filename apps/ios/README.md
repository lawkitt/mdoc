# mdoc for iOS

The iPhone and iPad app ([ADR 0085](../../docs/adr/0085-mobile-apps.md),
[spec](../../docs/specs/mobile.md)):
scan or open a document, review the names mdoc finds, share the
pseudonymized text to an AI app and restore its reply. SwiftUI on iOS 26
and later over the Rust core (`crates/mdoc-ffi`); no third-party Swift
packages.

```sh
just ios-app    # the core, then the app, extension and test targets for the Simulator
just ios-test   # unit, parity and UI tests on an iOS 26.2 Simulator
just ios-test "<Simulator name>"   # another device, such as an iPad
just ios-test "iPhone 17" mdocUITests/LivePreviewUITests   # only these (-only-testing)
just mobile     # what CI runs: --ffi in ci/macos, --ios (the iOS job) as ci/ios
just ios-device-test <hardware UDID>   # the device UI tests on a connected iPhone
```

They need Xcode 26.2 and the iOS 26.2 Simulator runtime of build 23C54, as
CI (`scripts/tools.env`; `scripts/ios-toolchain.sh` downloads the runtime,
about 8 GB, and `just doctor` checks both). Both build the core first
(`just ios-core`: `target/ios/MdocCore.xcframework`, its Swift bindings and
`pdfium.xcframework`; a few seconds when nothing changed). Builds go to
`target/ios/DerivedData` and are unsigned. The core is arm64 only, so the
Simulator builds exclude x86_64.

`just ios-core` also writes `target/ios/Build.xcconfig`: the build number
is the commit count up to HEAD and `MdocCoreCommit` is HEAD. A shallow clone
warns, and fails with `MDOC_REQUIRE_FULL_HISTORY=1`, which every TestFlight
build sets. A TestFlight build is a tagged `main`
commit ([ADR 0097](../../docs/adr/0097-ios-testflight-beta.md),
[release run](../../docs/release-run.md#ios-beta-build)). The app, the
Restore extension and both embedded frameworks carry a
`PrivacyInfo.xcprivacy` (no tracking, no collected data, the required-reason
APIs they call); `Info.plist` answers export compliance
(`ITSAppUsesNonExemptEncryption` NO: the core's AES-GCM is a standard
algorithm, and off the App Store in France it needs no documentation).

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
long job through the real background task. Run them before every TestFlight
build, with the phone connected, unlocked, online and with UI Automation on
(Settings → Developer). Each run downloads text recognition (18 MB); the
detector tier reuses the phone's own name detector or downloads it once
(about 630 MB):

```sh
just ios-device-test <hardware UDID>
just ios-device-test <hardware UDID> below-floor   # or detector: not by the phone's model
```

`scripts/ios-device-test.sh` copies the fixtures into the app's container
with `devicectl` (the app ships none), runs `ReviewFlowUITests` and the
tier's test, and fails on a new `mdoc-*.ips` crash report. The tier comes
from the phone's model: the iPhone 15 Pro and later run
`DeviceLongJobsUITests` (real downloads, Find names while the test leaves
the app past iOS's grace); earlier phones, or `below-floor`, run
`DeviceBelowFloorUITests`. Results go to `target/ios/device-test-*.xcresult`.

### Audits and timing

With `TEST_RUNNER_MDOC_A11Y_EXPLORE=1`: `AccessibilityAuditExploreUITests`
prints each issue (`A11Y[screen] …`) without failing, a guide rather than a
gate, because iOS 26's translucent sheets give false contrast reports;
`LivePreviewUITests/testTheAccessibilityAuditInEditMode` and
`EditToolbarUITests/testTheAccessibilityAuditWithTheToolbarUp` fail on any
issue. Editing's budgets are a keystroke's restyle under 8 ms and a structural
core call under 16 ms on the 37-page legal opinion; `LivePreviewUITests`
measures both nightly (`TEST_RUNNER_MDOC_TIMING_TESTS=1`) without a
threshold. `LargeTextExploreUITests` saves the main screens at the largest text
size into `TEST_RUNNER_MDOC_SHOTS_DIR`.

## Layout

The project is written by hand (`objectVersion` 77): each folder below is a
synchronized group, so adding a file needs no project edit.

| Folder | Target | What it holds |
| --- | --- | --- |
| `MdocFFI/` | `MdocFFI` framework | Compiles the generated bindings (`target/ios/swift/mdoc_ffi.swift`) over `MdocCore.xcframework`, plus `CoreFailure`, a public form of the core's errors (`CoreError` and `ReviewError`, one case each) |
| `Shared/` | app and extension | `Core/`: the app's `Library`, `DocumentSession` and `Setup` protocols and value types, `RealCore` over the bindings, `CoreFactory`; `Storage/` (folders, the case key); the restore view and view model; `Localizable.xcstrings` |
| `mdoc/` | app | `App/`: the app model, the navigator and its one presenter, the open documents' models, the UI tests' seed; `Library/`: the library model, file intake, the sidebar, Start, Cases and Documents; the document screen, hand-off, setup and the background model downloader, Settings, the long-job runner |
| `RestoreExtension/` | Restore with mdoc | The share extension's entry point |
| `mdocTests/`, `mdocUITests/` | tests | View models on the real core over temporary storage, with the fixtures and test doubles in `mdocTests/Support`; `RealCore`, the downloader and the Keychain store; parity with the desktop's goldens; one open → review → share → restore walk and the screenshots on the core |
| `Config/` | — | Info.plist additions and entitlements |

The app embeds `MdocFFI.framework` and `pdfium.framework`; the extension
links the app's copy of `MdocFFI`. The core reaches ONNX Runtime through
`OrtGetApiBase` in the image that holds it, so `MdocFFI` keeps that symbol
(`-u _OrtGetApiBase`), strips local symbols only and builds no debug dylib.
PDFium's prebuilt binary has no DWARF, so its dSYM in `pdfium.xcframework`
holds the symbol table only: PDFium frames in crash reports get function
names but no file or line.

Archiving in Xcode syncs the `InfoPlist.xcstrings` catalogs from Info.plist
(its spacing, a `comment` per key, extracted `CFBundleName`); they are
committed as Xcode writes them, so an archive leaves the checkout clean.

## Rules

The UI and app-structure rules are in
[docs/specs/mobile.md](../../docs/specs/mobile.md). Beside them:

- **Saving.** A review tap saves on a utility queue once taps pause
  (`SaveScheduler`, the core's `save_pending`). `CoreFactory` opens
  `RealCore` for the app and the extension.
- **Edit mode types into the core.** `TypingBridge` sends each committed
  edit, never marked IME text, to the session (`type`); its mirror of the
  core's text checks UIKit's edits, so the view's string stays the core's.
  Return, ⌫ and ⌦ ask the editor core (`StructuralKeys`). The view is
  restyled in place, never given new text: each keystroke restyles its block
  and the caret's lines (`LivePreview`, `markdown_block_styling`), with the
  caret's markers dimmed (`editor_visible_markers`); the caret never rests
  in a hidden run (`CaretSnap`). Cut and Copy use `InAppClipboard`; Paste
  reads it first, unless another app copied since, then the system
  clipboard.
- **Offsets are source bytes.** `TextOffsets` converts the core's UTF-8
  offsets for UIKit.
- **Markdown.** `MarkdownStyling` asks the core for spans;
  `MarkdownStyler` maps them to fonts and colours.
- **One long job at a time**, kept running in the background by
  `BGContinuedProcessingTask`. It reports a fraction, since iOS stops a
  continued task that shows no progress. When the system refuses the task,
  the job gets the short background grace, then cancels, and the app offers
  to resume on return. While a job runs, review waits: a
  Replace would make the core throw the scan away.
- **Locked phone.** A job pauses on lock. A result or save that comes while
  locked stays in the session and is saved on unlock (`ProtectedData`,
  `Library.saveDeferred`), because Data Protection Complete blocks writes.
- **OCR keeps finished pages** (`import_file_in`, the core's `imports/`);
  Resume recognition reads only the pages left. Dropping an import or
  deleting a document goes through the core (`discard_import`,
  `delete_document`).
- **Storage.** Everything lives in the App Group container
  (`group.io.github.lawkitt.mdoc`), excluded from backups; all but models use
  Data Protection Complete. The case key is in the Keychain's shared access
  group, this device only.
- **Intake.** PDF, DOCX and pasted text go to the core as they are; camera
  and photo pages become upright JPEGs, several pages one image-only PDF.
- **Downloads.** A background `URLSession`; "Waiting for Wi-Fi" comes from
  `NWPathMonitor`, because background sessions never call
  `taskIsWaitingForConnectivity`.
- **Handlers the system calls on its own queues** carry no actor
  isolation, because a main-actor check there crashed every long job on a
  phone.
- **Drawers.** `SideDrawer`'s drag is one UIKit pan moving layers by
  transform; it changes no shadow, mask or SwiftUI state per frame, because
  that lagged. The panel stays built while closed. An `.inspector` never
  goes inside the drawer's hosting controller, because it reset its child's
  traits without end.
- **Edit toolbar.** `EditToolbar`'s host never becomes first responder, and
  the text keeps room below the caret for it, so the caret's line stays
  above it.
- **Menus.** In a system menu an alias is `unbrokenAlias` (whole up to 12
  characters, else broken only after a separator) and the row's icon
  carries the category color, because a system menu draws every row's words
  in one color. A `Menu` or toolbar item uses `primaryGlass()` /
  `secondaryGlass()`, because it takes only the system's styles.
- **Candidate links** never show the system's link menu, in any state,
  because it exposed the raw `mdoc-candidate://` URL and Share.
- **No cross-fade.** Entering or leaving Adjust Words or Select never
  cross-fades changed text, because both layouts showed at once.
- **Table cells.** The text view drags a cell selection's handles itself,
  because UIKit's gesture hit-tests the table's hidden source.
- **Tips.** TipKit decides when a hint shows (display frequency
  immediate); mdoc draws it, never as `TipView` or `.popoverTip`. UI tests
  hide every tip (`Tips.hideAllTipsForTesting()`) unless
  `MDOC_SHOW_TIPS=1`. Reset tips resets each tip's eligibility, because
  `Tips.resetDatastore()` works only before TipKit starts.
- **Strings** live in `Shared/Localizable.xcstrings` (Info.plist strings in
  each target's `InfoPlist.xcstrings`).

## UI test launch

UI tests launch the app with `-mdoc-ui-testing` (`LaunchConfiguration` reads
every flag once): its own library, case key and preferences, below the
detector floor. It imports `MDOC_SEED_FILE` and scans it with the structured
rules alone, the only such scan, so review needs no model. On a phone the
seed reads the copy `just ios-device-test` put in the container.
`MDOC_KEEP_LIBRARY=1` keeps the library across launches; `MDOC_OCR_TESTS=1`
runs the tests that download the OCR model (with `MDOC_REAL_DOWNLOADS=1`);
`MDOC_UI_DETECTOR_TIER=1` gives the detector tier.
