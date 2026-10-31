# Android app

Status: Accepted, 2026-10-31.

## Problem

[ADR 0085](0085-mobile-apps.md) starts Android only after iOS 0.1 is on the
App Store and its first feedback has shaped the review screen. The
maintainer wants Android now, beside iOS 0.1. The work is planned on the
Windows PC, which has no Android toolchain, and the maintainer has no
Android phone: a friend's phone can run a build about once a week,
remotely. Some of ADR 0085's Android choices also conflict with its own
invariants or with Google Play's rules. Findings:
[Android grill](../design/android-grill-2026-10-31.md).

## Decision

- **Android starts now, beside iOS 0.1.** This replaces ADR 0085's "after
  iOS 0.1 ships". Shared with iOS: the Rust core and the `mdoc-ffi` API
  (`Library`, `DocumentSession`, `Setup`), the invariants of ADR 0085 and
  [ADR 0088](0088-ios-on-real-devices.md), the detector budgets, the
  screens and flows as polished by [ADR 0087](0087-ios-ui-polish.md), and
  the English strings word for word. Android's own: Material 3 idioms,
  navigation and back, the system share sheet and photo picker, and
  layout. iOS feedback on the review screen applies to both apps. Core and
  FFI changes for Android are additive and leave desktop and iOS green.
- **Android 12 (API 31) minimum, target API 36.** Raised from ADR 0085's
  API 29 for `dataExtractionRules`. Release builds are `arm64-v8a` only;
  debug builds add `x86_64` for the emulator.
- **Toolchain on Windows, scripted and pinned.** The Android SDK command-line
  tools and pinned `sdkmanager` packages on `E:\Android\Sdk`, NDK r29 and
  Temurin 21, each verified by size and digest (SHA-1 where Google
  publishes no SHA-256); the Gradle wrapper with `distributionSha256Sum`.
  `scripts/build-android-core.sh` builds `mdoc-ffi` as a `cdylib` with
  plain cargo and the NDK's clang, generates the Kotlin bindings with the
  existing `uniffi-bindgen`, and places ONNX Runtime 1.27.0 (Microsoft's
  Maven AAR) and PDFium 7988 (bblanchon) beside it, each SHA-256 pinned,
  with every `.so` aligned for 16 KB pages. Gradle only consumes
  `target/android/`. Android Studio is optional. Every recipe runs in Git
  Bash; nothing needs WSL.
- **Runtimes ship in the APK.** A `cfg(target_os = "android")`
  `linked_runtimes` takes the app's native library directory; nothing is
  downloaded but models.
- **Gradle dependencies are few and verified**: an exact-pinned version
  catalog, Gradle dependency verification with SHA-256, Dependabot weekly.
  No Firebase, Play services, OkHttp or analytics libraries in the app.
- **Real devices in four tiers**: the emulator on the PC daily; a phone of
  the maintainer's with 8 GB or more on the PC by USB; Firebase Test Lab
  physical phones (free tier) for the spike and the device tests at 6, 8
  and 12 GB or more; and the friend's phone weekly for the device checklist
  by hand. Test Lab sees public fixtures only. This replaces the mobile
  grill's "no Firebase Test Lab for now"; Test Lab shuts down on 2027-09-30
  and is revisited before then. Every long job runs on a real phone in the
  first week of app work.
- **The feasibility spike repeats on Android**: a throwaway
  instrumentation app in `tools/android-spike` runs `spike::run` and writes
  the iOS report format, on the owned phone and Test Lab phones. Peak
  memory is the process's peak resident set (`VmHWM`) at 2 GiB or less;
  serious throttling is thermal status SEVERE or higher; the report adds
  total and available memory and whether the process was killed. Android
  has its own floor constant: the lowest RAM class where every budget
  passes and the app is not killed. Because Android phones of one RAM class
  differ widely in speed (the iPhone 12 Pro met memory but missed the scan
  budget), setup also times one short detector run on the phone before
  offering the model download; a phone too slow for the 20-page budget is
  treated as below the floor. UI work runs beside the spike.
- **ADR 0088 holds on Android.** No rules-only tier: a phone below the
  floor installs (no Play device-catalog RAM exclusion), imports,
  recognizes text, marks by hand and says it cannot find names. A model
  download on Wi-Fi requires a Wi-Fi or Ethernet transport, never the
  unmetered flag (Android 11+ marks some unlimited 5G temporarily not
  metered); cellular only after a yes; a held download says "Waiting for
  Wi-Fi" with Use cellular. "Import not finished" and the + menu order
  (Paste, Files, Photos, Scan, the camera furthest from the button) carry
  over.
- **Long jobs run in one `dataSync` foreground service**, one job at a
  time, started only while a screen is visible, with a progress
  notification and Cancel. Notification permission is asked at the first
  long job, and the job runs if it is refused. A killed process offers to
  resume on return (OCR keeps finished pages; Find names reruns).
- **Model downloads run in Kotlin inside that service**, on the Wi-Fi
  `Network` object so they cannot fall back to cellular, resumable; Rust
  verifies and installs. Not `DownloadManager`.
- **Case key**: 32 random bytes wrapped by an Android Keystore AES key that
  needs an unlocked device, in StrongBox when present, without user
  authentication; the wrapped key sits in no-backup app storage. Documents
  stay in app-private credential-encrypted storage.
- **Nothing in backups**: backups are off, and `dataExtractionRules`
  excludes every domain from cloud backup and device-to-device transfer.
  Settings says a new phone starts empty.
- **Scan uses the system camera, not ML Kit.** + › Scan takes a page per
  system camera photo (Add page, Done), so the app needs no camera
  permission; PP-OCR recognizes it. This replaces ADR 0085's ML Kit
  Document Scanner, whose libraries report usage to Google and which needs
  Play services. No edge crop in 0.1.
- **Entry points**: Restore with mdoc is an activity that takes shared
  text, in the app's own process, with iOS's rules (the last-shared case
  with a picker, never mixing cases, Copy, Share, Save to document).
  Sharing or opening a PDF, DOCX or image imports it.
- **Review text is an Android `TextView` inside Compose**, as iOS uses
  `UITextView`: the core's Markdown spans, no hyphenation, candidates as
  clickable spans TalkBack reads and acts on, read-only selection with
  Replace…; restored replies too. Everything else is Compose. The core's
  UTF-8 offsets map to UTF-16 indices in Kotlin once per text; the FFI is
  unchanged and the Markdown is never re-rendered. ADR 0087's dark-mode
  contrast rules apply.
- **Tests**: JVM parity tests on the PC call a Windows build of `mdoc-ffi`
  through the same Kotlin bindings, on the Swift suite's fixtures and
  goldens, including Copy for AI bytes against desktop; one emulator test
  walks open, review, share, restore. `just android-device-test [serial]`
  runs the review flow and long jobs (Home and a process kill during a
  job) on a USB phone or Test Lab, by hand before every release after the
  first. `just android-survey <out>` takes every screen, light, dark and
  the largest font, on emulator phone and tablet; not in local CI.
- **Local CI**: `local/windows` runs the Android job after lint and test
  when a PR touches `crates/`, `Cargo.lock` or `apps/android/`: the core,
  the app, the JVM tests, then the emulator tests headless. The Mac keeps
  its Android `cargo check`.
- **Crash reports (ADR 0047)**: uncaught Kotlin exceptions, and at the next
  launch `ApplicationExitInfo`'s native crashes, low-memory kills and ANRs,
  show with Share report. Nothing is sent on its own.
- **Strings** in `strings.xml`, by hand; `scripts/mobile-strings.sh`
  reports English strings found on one platform only.
- **Distribution**: application ID `io.github.lawkitt.mdoc`; one app
  signing key, kept outside the repository, signs the GitHub APK and is
  uploaded to Play App Signing, so users can move between channels. A
  personal Play account (lawkitt is not a legal entity), so production
  waits for a 14-day closed test with 12 testers. The GitHub APK ships
  first. `android-v*` tags from `main`, `versionCode` a counter, the core
  commit recorded. Play Data safety: no data collected.

## Invariants

- ADR 0085's invariants hold on Android: the same document and model give
  the same candidates, aliases and restored text as desktop; no network
  except verified model downloads; no downloaded code; shared text passes
  through Copy for AI's export path.
- No Google SDK that sends data ships in the app.
- A foreground service starts only from a visible screen, with its
  declared type.

## Consequences

- The roadmap gains an Android 0.1 section.
- Platform acceptance gains section 14, "Android app", rows A1–A17 after
  P1–P17.
- Without a 12-tester closed test, Play production slips and the GitHub
  APK carries 0.1.
- Test Lab's shutdown in 2027 leaves the owned phone and the friend's as
  the real devices unless a replacement is found.
