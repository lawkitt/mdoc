- **CI: iOS tests on `macos-26`, builds on `xcode-27`.** The queue's iOS
  tests run with Xcode 26.2 on the iOS 26.2 Simulator, and the archive and a
  nightly test pass with Xcode 27.2 on iOS 27.2: two pins in
  `scripts/tools.env`, which `MDOC_XCODE` picks
  ([ADR 0124](docs/adr/0124-ios-tests-on-xcode-26-builds-on-xcode-27.md)).
  `main` keeps a built iOS core for each Xcode.
