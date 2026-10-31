# Android app grill, 2026-10-31

Interview record for Android 0.1: the Compose app on the shared Rust core
([ADR 0085](../adr/0085-mobile-apps.md)), started beside iOS 0.1 rather than
after it. Decisions land in ADRs once confirmed; this file keeps the research
and the design tree. Hub: the Windows PC (`windows`).

## Research facts

### The repository today

- `crates/mdoc-ffi` builds as `lib` and `staticlib` (iOS links the static
  library). Android loads a shared library through JNA, so it needs
  `cdylib` as well.
- UniFFI 0.32; `src/bin/uniffi-bindgen.rs` behind the `bindgen` feature
  generates Swift today and generates Kotlin the same way.
- ONNX Runtime is `ort =2.0.0-rc.13` with `load-dynamic` everywhere. Desktop
  downloads the runtime; iOS links it statically and finds `OrtGetApiBase`
  in its own image (`mdoc_import::ocr::linked_runtimes`, `cfg(target_os =
  "ios")`). Android has no such path yet: `linked_runtimes()` returns an
  error off iOS.
- The only `cfg(target_os = ...)` mobile code is iOS: the DOCX preview
  worker is excluded on iOS (`mdoc-import`), and the spike's memory reading
  is `phys_footprint`.
- `just mobile` (`scripts/mobile-check.sh`, macOS only) `cargo check`s
  `mdoc-ffi` for `aarch64-linux-android` with NDK r29 (API 29 clang).
  `scripts/install-android-ndk.sh` installs NDK r29 on macOS only, verified
  by size and SHA-1 (Google publishes no SHA-256).
- `Library::new(root, case_key)` takes the 32-byte case key from the
  platform; the core never touches a keychain.
- iOS bundle ID: `io.github.lawkitt.mdoc`.
- ADR 0088 (iOS on real devices) is open in lawkitt/mdoc#291, not on `main`.
- iOS spike, iPhone 17 Pro: detector peak 1.97 GiB of the 2 GiB budget,
  cold load 0.91 s, 20 pages in 43.2 s, thermal nominal then fair. The
  12 Pro row is pending.

### This PC

- No Android SDK, Studio, NDK or `adb`. OpenJDK 22.0.2 on `PATH`.
- Rust 1.99.0 (pinned by `rust-toolchain.toml`), only the
  `x86_64-pc-windows-msvc` target installed.
- Hyper-V and virtualization-based security are on, so the emulator runs
  on the Windows Hypervisor Platform (AMD CPU).
- E: Dev Drive: 314 GB free. `local/windows` runs from `D:\ci\mdoc`.

### Upstream pieces

- **ONNX Runtime for Android**: `com.microsoft.onnxruntime:onnxruntime-android`
  on Maven Central has 1.27.0 (desktop's version) and later. The AAR holds
  `libonnxruntime.so` per ABI.
- **PDFium for Android**: bblanchon/pdfium-binaries `chromium/7988` (the
  version iOS uses) has `pdfium-android-arm64.tgz` and
  `pdfium-android-x64.tgz`.
- **NDK**: r29 has a `windows.zip`.
- **Google Play**: new apps and updates must target API 36 (Android 16)
  from 2026-08-31. A personal developer account made after 2023-11-13 must
  run a closed test with at least 12 testers opted in for 14 days in a row
  before production; organization accounts are exempt. Apps targeting
  Android 15+ must support 16 KB memory pages (every `.so` aligned).
- **Android developer verification**: from 2026-09-30, certified phones in
  Brazil, Indonesia, Singapore and Thailand need a verified developer for a
  sideloaded APK (unverified ones need the advanced flow or `adb`); global
  in 2027. A GitHub Releases APK is affected.

### Found in round 1

- **No phone on this PC.** The maintainer has no Android phone. A friend's
  phone can run a build about once a week, remotely; it cannot reach this
  PC by USB.
- **Play account needs a phone once.** A new personal account must verify,
  through the Play Console app on a non-rooted phone on Android 10 or later,
  that its owner has an Android phone; it takes under a minute, the phone
  need not be kept, and Google may ask again later on any phone.
  Organization accounts are exempt from this and from the 12-tester test.
- **Firebase Test Lab** runs instrumented tests on real phones in Google's
  lab. Spark (free, no card): 5 physical and 10 virtual runs a day. Blaze:
  30 free physical minutes a day, then $5 per device-hour. Android Device
  Streaming (an interactive real phone in Android Studio, with `adb`): 30
  free minutes a month per project, then $0.15 a minute. Test Lab is
  deprecated and shuts down on 2027-09-30.
- **ML Kit sends Google metrics.** Every ML Kit Android SDK, the Play
  services ones included, collects device and app information, a
  per-installation identifier, performance metrics, API configuration and
  error codes for diagnostics and usage analytics, and the Data safety form
  must account for it. The Document Scanner's models, logic and UI are
  downloaded by Google Play services, not shipped in the app.
- **The core's offsets are UTF-8 bytes** (`TextRange`); iOS converts them in
  Swift (`DocumentViewModel`). Kotlin strings are UTF-16.

## Design tree

- Round 1: sequencing and what Android may change; ADR 0088's PR; minimum
  and target API; toolchain shape; Rust build; ABIs; devices; local CI
  machine; Play account; signing.
- Later rounds: the spike on Android (metric, harness, floor), ADR 0088 on
  Android, long jobs, downloads and the cellular rule, Keystore and data
  protection, scanner, share target, backups, the review screen in Compose,
  parity tests, the UI survey, device tests, versions and the roadmap.

## Decisions

### Round 1

1. **Android starts now, beside iOS 0.1.** ADR 0089 replaces ADR 0085's
   "after iOS 0.1 ships". Shared: the Rust core and the `mdoc-ffi` API, the
   invariants of ADR 0085 and 0088, the detector budgets, the screens and
   flows as polished by ADR 0087, and the English strings word for word.
   Android's own: Material 3 idioms, navigation and back, the system share
   sheet and photo picker, layout. iOS feedback on the review screen later
   applies to both. Android's core and FFI changes are additive and leave
   desktop and iOS green.
2. **lawkitt/mdoc#291 (ADR 0088) merges first**, from the Mac; until then
   ADR 0089 is written against its text.
3. **Target API 36, minimum Android 12 (API 31)**, raised from ADR 0085's
   API 29 for `dataExtractionRules`.
4. **Scripted, pinned toolchain.** `scripts/install-android-sdk.sh`
   installs the command-line tools and pinned `sdkmanager` packages on
   `E:\Android\Sdk`, verified by size and SHA-1; `install-android-ndk.sh`
   gains Windows; Temurin 21 pinned and verified for Gradle; the Gradle
   wrapper with `distributionSha256Sum`. Android Studio is optional and no
   build depends on it.
5. **Plain cargo** in `scripts/build-android-core.sh`, mirroring
   `build-ios-core.sh`: `mdoc-ffi` gains `cdylib`; NDK clang for API 31;
   `target/android/jniLibs/<abi>/`; Kotlin bindings from `uniffi-bindgen`;
   ONNX Runtime 1.27.0 from the Maven AAR and PDFium 7988 from bblanchon,
   each SHA-256 pinned; a `cfg(target_os = "android")` `linked_runtimes`
   that takes the app's `nativeLibraryDir`; a 16 KB alignment check. Gradle
   only consumes `target/android/`. Every recipe runs in Git Bash.
6. **Release ABI `arm64-v8a` only**; debug adds `x86_64` for the emulator.
7. **Devices: no phone here** (see the facts above); the plan is round 2.
8. **The Android job runs on `windows`** inside `local/windows`, after lint
   and test, when a PR touches `crates/`, `Cargo.lock` or `apps/android/`:
   core, app, JVM tests, then emulator tests headless, the emulator shut
   down after. The Mac keeps its Android `cargo check`.
9. **No Play account yet**; the maintainer can make one. Type and testers
   are round 2.
10. **Application ID `io.github.lawkitt.mdoc`; one app signing key** for Play
    (uploaded to Play App Signing as "use my own key", with a separate
    upload key) and the GitHub APK. Generated on this PC outside the
    repository; its password lives in the maintainer's password manager.

### Round 2

11. **Real devices in four tiers**: the emulator here daily; Firebase Test
    Lab physical phones (Spark, free) for the spike and the device tests,
    chosen by RAM class (6, 8, 12 GB or more); Android Device Streaming's
    free 30 minutes a month for interactive checks; the friend's phone
    weekly for the device checklist by hand. Test Lab sees public fixtures
    only, and the app holds no Firebase code. This reverses the mobile
    grill's "no Firebase Test Lab for now"; revisit before Test Lab shuts
    down on 2027-09-30.
12. **The friend's run**: a release-signed APK the maintainer sends; back
    come screenshots and the app's local crash report (ADR 0047), so the
    app gets a Share report button. After Play exists, Play internal
    testing replaces the file. No remote `adb` on the friend's phone.
13. **Personal Play account.** lawkitt is not a legal entity, so no
    organization account. How to pass the device check is round 3.
14. **ADR 0088 carries over whole**: no rules-only tier, and below the
    floor the app installs, imports, recognizes, marks by hand and says it
    cannot find names (no Play device-catalog RAM exclusion, which the APK
    could not match); Wi-Fi means a Wi-Fi or Ethernet transport, never the
    unmetered flag (Android 11+ marks some unlimited 5G temporarily not
    metered); "Waiting for Wi-Fi" with Use cellular; "Import not
    finished"; + menu Paste, Files, Photos, Scan with the camera furthest
    from the button. #290's Android form: a foreground service started from
    the background or without its declared type crashes; long-job services
    start only from a visible screen.
15. **Long jobs in one `dataSync` foreground service**, one job at a time,
    started only on screen; the notification shows progress and Cancel;
    notification permission asked at the first long job, and the job runs
    if it is refused; a killed process offers resume on return (OCR keeps
    pages, Find names reruns); Android 15's 6-hour limit stops cleanly.
16. **Model downloads in Kotlin inside that service**, opened through the
    Wi-Fi `Network` object so they cannot fall back to cellular; cellular
    only after a yes, bound to the cellular network; resumable with Range;
    Rust verifies and installs. Not `DownloadManager`.
17. **Case key**: 32 random bytes wrapped by an Android Keystore AES key
    that needs an unlocked device, StrongBox when present, no user
    authentication; the wrapped key in no-backup app storage. Documents stay
    in app-private credential-encrypted storage. P15 for Android reads
    "unreadable before first unlock and in any backup".
18. **No ML Kit Document Scanner in 0.1.** Scan opens the system camera
    per page (Add page, Done), so the app needs no camera permission;
    PP-OCR recognizes; no edge crop. An own crop screen may follow. This
    replaces ADR 0085's ML Kit scanner: ML Kit reports usage to Google and
    needs Play services.
19. **Entry points**: Restore with mdoc is an activity that takes shared
    text, in the app's own process (no App Group or keychain group), with
    iOS's rules (last-shared case, picker, never mixes cases, Copy, Share,
    Save to document). Sharing or opening a PDF, DOCX or image imports it.
20. **Backups off**: `allowBackup="false"` and `dataExtractionRules`
    excluding every domain from cloud backup and device transfer; models
    in the no-backup folder; Settings says a new phone starts empty.
21. **Review text is a `TextView` in Compose** (as iOS uses `UITextView`):
    the core's spans, no hyphenation, candidates as clickable spans TalkBack
    reads, read-only selection with Replace… in its action mode. Restored
    replies too. Everything else is Compose. A Kotlin converter maps UTF-8
    offsets to UTF-16 indices once per text, JVM-tested with Cyrillic, emoji
    and grapheme clusters; the FFI is unchanged; Markdown is never
    re-rendered.
22. **Parity tests**: JVM tests on the PC through the same Kotlin bindings
    and a Windows build of `mdoc-ffi`, on the Swift suite's fixtures and
    goldens, including Copy for AI bytes against desktop; one emulator test
    walks open, review, share, restore.
23. **Spike metrics**: peak resident memory (`VmHWM`, read by the core) ≤ 2
    GiB; the report adds total and available memory at start and whether
    the process was killed. Serious throttling is thermal status SEVERE or
    higher. Android gets its own floor constant: the lowest RAM class where
    every budget passes and the app is not killed.
24. **UI survey**: `just android-survey <out>`, a screenshot test on the
    emulator (a Pixel-class phone and Pixel Tablet; light, dark, largest
    font), not in local CI; before and after sets under
    `docs/evidence/android-app/`.

### Found in round 2

- No official way around the device check: it needs a non-rooted physical
  phone on Android 10 or later; an emulator does not count. Third-party
  guides suggest borrowing or buying one.

### Round 3

25. **Buy a used phone with 8 GB or more** (a Pixel 7 Pro or Pixel 8 class)
    and keep it on this PC by USB: it passes the Play device check, runs the
    spike on hardware we control, and checks every long job in the first
    week. Test Lab covers the other RAM classes.
26. **The spike**: a throwaway instrumentation app in `tools/android-spike`
    calling `spike::run` and writing the iOS JSON;
    `scripts/android-spike-device.sh` runs it on the owned phone by USB and
    on Test Lab phones at 6, 8 and 12 GB or more, with public fixtures and
    models pushed with the test; the report joins
    `docs/evidence/mobile/<date>/`. UI work runs beside it; the budgets
    decide what ships. The maintainer installs `gcloud`, signs in and makes
    the free Firebase project.
27. **Device UI tests**: `just android-device-test [serial]`, a USB phone
    with a serial and Test Lab without: .docx and text-PDF import, a scan
    with a real OCR download on Wi-Fi, Find names, Home during a job, and a
    process kill during a job (resume on return). By hand before every
    release after the first; not in local CI.
28. **Platform acceptance section 14, "Android app"**: rows A1–A17 matching
    P1–P17 (TalkBack, font scale, P15's Android wording), each marked
    *Emulator* or *Test Lab* (the agent) or *Phone* (the maintainer or the
    friend).
29. **Gradle**: an exact-pinned version catalog (AGP, Kotlin, Compose BOM,
    AndroidX core, activity, lifecycle, Material 3, JNA); Gradle dependency
    verification with SHA-256 in `verification-metadata.xml`; Dependabot
    gains `gradle` for `/apps/android`, weekly. No Firebase, Play services,
    OkHttp or analytics libraries.
30. **Crash reports (ADR 0047)**: uncaught Kotlin exceptions are written to
    app storage; at the next launch `ApplicationExitInfo` adds native
    crashes, low-memory kills and ANRs with their trace; they show then with
    Share report. Nothing is sent on its own.
31. **Strings**: `strings.xml` by hand; `scripts/mobile-strings.sh` reports
    English strings found on one platform only, without failing.
32. **Versions and releases**: `android-v0.1.0` tags from `main`;
    `versionName` 0.1.0, `versionCode` a counter, the core commit recorded;
    `just android-release` makes the signed Play bundle and the signed
    arm64 APK with its SHA-256 for GitHub Releases. Channels: GitHub APK,
    then the Play closed test, then production; RuStore on request.
    Fragments in `changes/` lead with "Android:". Data safety: no data
    collected.
33. **Closed-test testers are recruited now**: Android users among the
    TestFlight lawyers, friends and family, the friend included. Without
    12, Play production slips and the GitHub APK carries 0.1.
34. **Every step on the maintainer's side comes as exact instructions**
    (below), kept current as scripts land.

The frontier is empty, pending the maintainer's confirmation of the roadmap
section.

## Maintainer actions

In order of need. Git Bash commands are in `bash` blocks, PowerShell ones
in `powershell` blocks. Passwords are typed only into the prompt that asks
for them and kept in the password manager; never in chat or files.

### Now

1. **Merge lawkitt/mdoc#291 (ADR 0088)** from the Mac.
2. **Buy the phone**: a used, unlocked Pixel 7 Pro (12 GB) or Pixel 8
   (8 GB), not rooted, with a USB-C data cable.
3. **Make the Play developer account** (personal) at
   <https://play.google.com/console/signup>: pay the $25 fee and finish the
   identity check with your ID. The phone check waits for the phone (below).
4. **Make the Firebase project for Test Lab**: at
   <https://console.firebase.google.com>, Create a project named
   `mdoc-test-lab`, Google Analytics off, Spark plan (no billing account).
   Note its project ID.
5. **Install and sign in to `gcloud`**, in PowerShell:

   ```powershell
   winget install --id Google.CloudSDK --exact
   ```

   Then in a new Git Bash window (the browser opens for the sign-in;
   replace `<project-id>` with the ID from step 4):

   ```bash
   gcloud auth login
   ```

   ```bash
   gcloud config set project <project-id>
   ```

   ```bash
   gcloud services enable testing.googleapis.com toolresults.googleapis.com
   ```

   ```bash
   gcloud firebase test android models list --filter=form=PHYSICAL --limit=5
   ```

   The last command lists five lab phones when everything works.
6. **Collect 12 testers**: the Google account email of each Android user
   (the friend included) who agrees to stay opted in for 14 days.

### When the phone arrives

1. On the phone: Settings › About phone › tap **Build number** seven times;
   then Settings › System › Developer options › **USB debugging** on.
2. **Pass the Play device check**: in Play Console on the PC, open Home ›
   "Verify that you have access to an Android mobile device", scan its QR
   code with the phone, install the Play Console app, sign in as the
   account owner and tap Verify.
3. **Connect it to this PC** (after the toolchain PR lands), then accept
   the "Allow USB debugging" prompt on the phone:

   ```bash
   /e/Android/Sdk/platform-tools/adb devices -l
   ```

   The phone shows as `device`, not `unauthorized`.

### Before the first release

The toolchain PR installs Temurin 21 at `E:\Android\jdk-21`. Make the two
keys outside the repository; `keytool` asks for each password:

```bash
mkdir -p ~/keys/mdoc-android
```

```bash
/e/Android/jdk-21/bin/keytool -genkeypair -v -keystore ~/keys/mdoc-android/mdoc-app.jks -alias mdoc -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=mdoc, O=lawkitt"
```

```bash
/e/Android/jdk-21/bin/keytool -genkeypair -v -keystore ~/keys/mdoc-android/mdoc-upload.jks -alias upload -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=mdoc upload, O=lawkitt"
```

Copy both `.jks` files to an offline backup: a lost app key means the
GitHub APK can never be updated in place. `just android-release` asks for
the passwords when it signs. The Play App Signing upload of the app key
(Play's own encryption command) and Android developer verification for the
APK get their exact steps when the first release is near.
