- **Android toolchain on Windows.** `just android-setup` installs the pinned
  Android SDK, emulator, JDK 21 and NDK; `just android-emulator` boots an
  API 36 emulator with 16 KB pages; and `just android-core` builds the mobile
  core with ONNX Runtime and PDFium for Android phones and the emulator
  ([ADR 0089](docs/adr/0089-android-app.md)).
