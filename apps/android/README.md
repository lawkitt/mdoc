# mdoc for Android

The review-first Android app ([ADR 0089](../../docs/adr/0089-android-app.md)):
Jetpack Compose over the Rust core's Kotlin bindings (`crates/mdoc-ffi`),
the twin of [`apps/ios`](../ios/README.md). Android 12 (API 31) and later,
compiled against API 37, targeting API 36.

## Build and run

On the Windows PC, in Git Bash:

```sh
just android-setup          # once: pinned SDK, emulator, JDK 21, NDK under E:\Android
just android-core           # the core for arm64-v8a and x86_64, its Kotlin bindings, a desktop build for the JVM tests
just android-emulator       # boots the API 36 phone emulator (16 KB pages); prints emulator-5554
cd apps/android
export JAVA_HOME=E:/Android/jdk-21 ANDROID_HOME=E:/Android/Sdk
./gradlew installDebug      # then open mdoc on the emulator
```

`just android-emulator --window` shows the emulator instead of running it
headless.

## Tests

- `./gradlew testDebugUnitTest`: JVM tests on the PC, among them the parity
  tests, which call the desktop build of the core through the same Kotlin
  bindings and check the desktop goldens (Copy for AI bytes included).
- `ANDROID_SERIAL=emulator-5554 ./gradlew :app:connectedDebugAndroidTest`:
  the instrumented tests on the emulator, each on its own isolated core.
- `just android` runs both (with the builds) when a commit changes the core
  or the app: the Windows PC's local CI job, on its own emulator
  (`emulator-5560`).
- `longjobs/` is the long-job test: a self-instrumenting UiAutomator test
  that drives the debug app from its own process, so it can press Home and
  kill mdoc mid-job (the model download, text recognition, Find names), then
  checks mdoc's exit info for crashes and ANRs. It uses the app's real
  storage and downloads the models, so it is not in local CI:
  `ANDROID_SERIAL=<serial> ./gradlew :longjobs:connectedDebugAndroidTest`.
- `just android-device-test [<serial> | <model>:<api> ...]` runs both suites
  on a phone by its adb serial or on Firebase Test Lab phones.
- `just android-survey <out> phone|tablet` takes the UI survey's
  screenshots (`SurveyTest`, only with `mdoc.survey=1`).

## How it fits together

- `core/AppCore.kt` makes the core's `MdocLibrary` (cases, stored documents)
  and `Setup` (models). In Kotlin the core's `Library` is `MdocLibrary`
  (`crates/mdoc-ffi/uniffi.toml`).
- The case key is 32 random bytes wrapped by an Android Keystore key
  (`storage/CaseKeyStore.kt`); documents, cases and models stay out of
  backups and device transfers (`res/xml/data_extraction_rules.xml`).
- The APK extracts its native libraries: the core opens PDFium and ONNX
  Runtime by file path beside itself. The app sets `PDF_INSPECTOR_MODEL_CACHE`
  to its cache folder for OCR.
- Strings are in `res/values/strings.xml`, word for word with the iOS String
  Catalog.

## Dependencies

Exact pins in `gradle/libs.versions.toml`; Gradle checks every artifact's
SHA-256 against `gradle/verification-metadata.xml`. After changing a
dependency, regenerate it with the emulator running:

```sh
ANDROID_SERIAL=emulator-5554 ./gradlew --write-verification-metadata sha256 help testDebugUnitTest assembleDebug assembleDebugAndroidTest connectedDebugAndroidTest
```
