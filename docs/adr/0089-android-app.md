# Android app

Status: Accepted, 2026-10-31.

## Problem

The maintainer wanted Android beside iOS 0.1, and some of
[ADR 0085](0085-mobile-apps.md)'s Android choices conflicted with Google
Play's rules.

## Decision

- **Android starts beside iOS**, over the same Rust core and `mdoc-ffi` API,
  with the rules of `docs/specs/mobile.md`; Material 3 idioms are Android's
  own. Core and FFI changes for Android are additive.
- **Android 12 (API 31) minimum, target API 36**, for
  `dataExtractionRules`. Release builds are `arm64-v8a` only.
- **A scripted, pinned toolchain:** SDK, NDK, JDK, Gradle, ONNX Runtime and
  PDFium verified by digest; native libraries aligned for 16 KB pages.
- **Runtimes ship in the APK**; nothing is downloaded but models.
- **Few, verified Gradle dependencies:** an exact-pinned catalog with
  SHA-256 dependency verification. No Firebase, Play services, OkHttp or
  analytics libraries in the app.
- **Long jobs run in one `dataSync` foreground service** with a progress
  notification and Cancel; a refused notification permission doesn't stop
  the job.
- **Model downloads run in Kotlin inside that service**, on the Wi-Fi
  `Network` object so they cannot fall back to cellular, resumable; Rust
  verifies and installs. Not `DownloadManager`.
- **Case key:** 32 random bytes wrapped by an Android Keystore AES key
  (StrongBox when present, no user authentication), the wrapped key in
  no-backup storage. "Needs an
  unlocked device" applies from Android 15 only, because Android 12–14
  delete such keys when the screen lock is removed.
- **Nothing in backups or device transfers** (`dataExtractionRules`);
  documents stay in credential-encrypted app storage.
- **Scan uses the system camera, not ML Kit**, so no camera permission and
  no Play services; PP-OCR recognizes the pages.
- **Review text is an Android `TextView` inside Compose**, with the core's
  spans.
- **Tests:** JVM parity tests run a desktop build of `mdoc-ffi` through the
  Kotlin bindings on the shared goldens.
- **Real devices:** the emulator, Firebase Test Lab phones (public fixtures
  only) and a borrowed phone.
- **Distribution:** application ID `io.github.lawkitt.mdoc`, Google Play
  (Play App Signing, [ADR 0091](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0091-mobile-ci-and-releases.md)) and an APK
  on GitHub Releases. Play Data safety: no data collected.

## Invariants

- ADR 0085's invariants hold on Android.
- No Google SDK that sends data ships in the app.
- A foreground service starts only from a visible screen, with its declared
  type.

## Consequences

- A personal Play account needs a 14-day closed test with 12 testers before
  production, so the GitHub APK ships first.
- Test Lab closes on 2027-09-30.

References: [Android grill](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/design/android-grill-2026-10-31.md).
