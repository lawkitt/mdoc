- **Android scripts on macOS and Linux, and the minified build in CI.**
  `just android-setup`, `just android-emulator`, `just android-core` and
  `just android` run on an Apple Silicon Mac (an arm64-v8a emulator) and on
  Linux x86_64 with KVM as well as on Windows, each archive pinned per
  platform; on macOS and Linux the SDK goes in `mdoc-sdk`, apart from
  Android Studio's. The Android job also builds the minified release-like
  app (R8, `apps/android/app/proguard-rules.pro`) and opens its library on
  the emulator, so a missing keep rule fails it
  ([ADR 0091](docs/adr/0091-mobile-ci-and-releases.md)).
