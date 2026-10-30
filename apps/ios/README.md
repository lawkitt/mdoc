# mdoc for iOS

The iPhone and iPad app ([ADR 0085](../../docs/adr/0085-mobile-apps.md)):
scan or open a document, review the names mdoc finds, share the
pseudonymized text to an AI app and restore its reply. SwiftUI on iOS 17
and later; no third-party Swift packages.

```sh
just ios-app    # build the app, extension and test targets for the Simulator
just ios-test   # unit tests and the UI test on an iOS 26.1 Simulator
just ios-test "<Simulator name>"   # another device, such as an iPad
```

Both need Xcode with the iOS 26.1 platform; builds go to
`target/ios/DerivedData` and are unsigned.

## Layout

The project is written by hand (`objectVersion` 77): each folder below is a
synchronized group, so adding a file needs no project edit.

| Folder | Target | What it holds |
| --- | --- | --- |
| `Shared/` | app and extension | `Core/`: the Swift protocols mirroring `mdoc-ffi`'s `Library`, `DocumentSession` and `Setup`, their value types, and `FakeCore`; `Storage/`; the restore view and view model; `Localizable.xcstrings` |
| `mdoc/` | app | Home, the document screen, hand-off, setup, Settings and the long-job runner |
| `RestoreExtension/` | Restore with mdoc | The share extension's entry point |
| `mdocTests/`, `mdocUITests/` | tests | View models against `FakeCore`; one open → review → share → restore walk |
| `Config/` | — | Info.plist additions and entitlements |

## Rules

- **Rust owns state.** View models keep the snapshots the core returns and
  view state only; every change is a core call. `CoreFactory` is the one
  place that picks the core: `FakeCore` (fictional fixtures, in memory)
  until `mdoc-ffi` is wired in.
- **Offsets are source bytes.** Candidate and span ranges are UTF-8 offsets
  into the Markdown source; `TextOffsets` converts them for UIKit.
  `MarkdownSpanKind` is filled from `mdoc-markdown` later.
- **One long job at a time**, with progress and cancel. iOS 26 keeps it
  running in the background through `BGContinuedProcessingTask`; earlier
  versions get the short grace, then cancel and offer to resume.
- **Storage.** Everything lives in the App Group container
  (`group.io.github.lawkitt.mdoc`), excluded from backups; documents,
  Markdown and cases use Data Protection Complete.
- **The extension loads no model.** It only restores with a case's mapping.
- **Strings** live in `Shared/Localizable.xcstrings` (Info.plist strings in
  each target's `InfoPlist.xcstrings`); every visible string goes there.

The UI test launches the app with `-mdoc-ui-testing`: instant jobs, models
installed, a fresh fixture library.
