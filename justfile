# mdoc development commands. CI runs the same recipes (ADR 0037).
# Logic beyond a single command lives in scripts/*.sh.

set shell := ["bash", "-euo", "pipefail", "-c"]
# A bare `bash` on Windows can resolve to WSL; Git Bash sees the Windows toolchain.
set windows-shell := ["C:/Program Files/Git/bin/bash.exe", "-euo", "pipefail", "-c"]

# List recipes
default:
    @just --list --unsorted

# Format, licences/advisories, clippy and tests — what CI runs
check: fmt-check deny lint-scripts lint test

# Format the workspace
fmt:
    cargo fmt

# Fail on unformatted code
fmt-check:
    cargo fmt --check

# Dependency licences and advisories (cargo-deny); --locked so it never repairs
# a drifted Cargo.lock before `lint` checks it (ADR 0053)
deny:
    cargo deny --locked check

# The licence, ban and source checks alone, and the advisories alone. CI's lint
# runs the first on every change and the advisories when Cargo.lock or
# deny.toml changed (scripts/deny-ci.sh); the nightly run takes them daily, so
# a new advisory never turns every PR red (ADR 0095)
deny-policy:
    cargo deny --locked check bans licenses sources

deny-advisories:
    cargo deny --locked check advisories

# Cargo.lock must match Cargo.toml (ADR 0053); no `#[path]` under src/ (ADR 0069);
# no GPUI under the GUI-free crates (ADR 0069); then clippy with warnings as errors
lint:
    cargo metadata --locked --format-version 1 > /dev/null
    if grep -rnE --include='*.rs' '#\[path[[:space:]]*=' src; then echo 'error: #[path] under src/; use a folder module (ADR 0069)' >&2; exit 1; fi
    scripts/gui-free.sh
    cargo clippy --workspace --all-targets --features headless-renders -- -D warnings

# shellcheck and shfmt over scripts/; shfmt's style is in .editorconfig (ADR 0070)
lint-scripts:
    shellcheck -x -P SCRIPTDIR scripts/*.sh
    shfmt -d scripts

# Workspace tests through cargo-nextest (.config/nextest.toml) and doctests,
# with the script suites running beside them (ADR 0082)
test:
    bash scripts/test.sh

# One test tier (ADR 0095): unit, app, renders, recognition, scripts or quarantined
test-tier tier:
    bash scripts/test.sh --tier {{tier}}

# Isolated release-script regressions; no network, builds or real releases
test-release:
    bash scripts/test-release.sh

# Local CI runner and hosted fallback regressions; fake gh and just, no network or builds
test-local-ci:
    bash scripts/test-local-ci.sh
    bash scripts/test-local-ci-fallback.sh

# Local CI runner (ADR 0082): run, once, install, start, stop, status, uninstall
local-ci *args:
    scripts/local-ci.sh {{args}}

# Recognition fixtures (ADR 0093): annotate a file or folder, --gold, --snapshot, --scoreboard, --refresh-snapshots, --check (the detection gate)
[no-cd]
[positional-arguments]
recognize *args:
    @cargo run --locked --quiet --manifest-path "{{justfile_directory()}}/Cargo.toml" -p mdoc-detect --example recognize -- "$@"

# What mdoc's own import makes of a file, as Markdown on stdout: judge probes on this (ADR 0093)
[no-cd]
[positional-arguments]
import *args:
    @cargo run --locked --quiet --manifest-path "{{justfile_directory()}}/Cargo.toml" -p mdoc-import --example convert -- "$@"

# The detection gate (ADR 0093), macOS: the model over every recognition fixture when recognition changed; CI runs it sharded on hosted macos-26 (ADR 0095)
detection-gate:
    scripts/detection-gate.sh

# The detection gate's regenerated snapshots from a CI run (a PR's newest, or a run) into the tree, to review and commit (ADR 0095)
[positional-arguments]
snapshots-fetch pr-or-run:
    scripts/snapshots-fetch.sh "$1"

# Offscreen macOS/Windows screen/Tab evidence in a new output directory (dev only)
[positional-arguments]
headless-renders out:
    cargo run --locked --features headless-renders --bin mdoc -- --headless-renders "$1"

# Ignored performance tests, serially; run on an idle machine
perf:
    scripts/perf.sh

# Release gate: download the pinned OCR runtime and recognize EN/RU fixtures
smoke-ocr:
    scripts/smoke-ocr.sh

# Installed-model smoke tests; --download installs the default models first
smoke-models *args:
    scripts/smoke-models.sh {{args}}

# Everything, including the model downloads — before a release
check-all: check smoke-ocr smoke-models

# iOS core into target/ios: MdocCore.xcframework, Swift bindings, PDFium (ADR 0085)
ios-core:
    scripts/build-ios-core.sh

# Pinned Android SDK, emulator, JDK 21 and NDK under E:\Android, ~/Library/Android (Mac) or ~/Android (Linux) (ADR 0089, 0091)
android-setup:
    scripts/install-android-sdk.sh
    scripts/install-android-ndk.sh

# Boot the API 36 emulator (16 KB pages; arm64-v8a on the Mac, x86_64 elsewhere, Linux needs KVM) headless and print its adb serial; --window shows it (ADR 0089, 0091)
android-emulator *args:
    scripts/android-emulator.sh {{args}}

# The Android job: core, JVM parity tests, app, the minified app opening its library, emulator tests, when the android area changed; the PC's queue runner runs it as ci/android (ADR 0089, 0091, 0095)
android:
    scripts/android-check.sh

# Take the Android UI survey (every screen, light and dark, largest font) into a folder; kind: phone or tablet (ADR 0087, 0089)
android-survey out kind="phone":
    scripts/android-survey.sh "{{out}}" "{{kind}}"

# The app's tests and the long-job test on a phone by adb serial, or on Test Lab phones named model:api (default: one per RAM class) (ADR 0089)
android-device-test *devices:
    scripts/android-device-test.sh {{devices}}

# Android core into target/android: jniLibs for arm64-v8a and x86_64, Kotlin bindings (ADR 0089)
android-core:
    scripts/build-android-core.sh

# cargo check of mdoc-ffi for iOS and Android (--ffi), then the iOS job when the ios area changed (--ios); ci/macos and ci/ios run them (ADR 0085, 0095)
mobile *args:
    scripts/mobile-check.sh {{args}}

# Build this machine's installers into dist/
package:
    scripts/package.sh

# Release gate: OCR from the packaged app (signed .dmg / release mdoc.exe)
smoke-package:
    scripts/smoke-package.sh

# In-app update from an older signed .dmg to the latest release (ADR 0040)
smoke-update *args:
    scripts/smoke-update.sh {{args}}

# Release commit + tag; see `scripts/release.sh --help` for automation flags
release version *flags:
    scripts/release.sh {{version}} {{flags}}

# Build the iOS app, its extension and test targets on the core for the Simulator, unsigned (ADR 0085)
[macos]
ios-app: ios-core
    xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination 'generic/platform=iOS Simulator' -derivedDataPath target/ios/DerivedData -quiet build-for-testing

# Build the iOS app and its extension in the Release configuration for the Simulator, unsigned, without tests: what an archive compiles (ci/ios-release)
[macos]
ios-release: ios-core
    xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -configuration Release -destination 'generic/platform=iOS Simulator' -derivedDataPath target/ios/ReleaseData -quiet build CODE_SIGNING_ALLOWED=NO

# Run the iOS unit, parity and UI tests on a Simulator device of the pinned runtime (scripts/tools.env), without the quarantined ones (.config/quarantine.txt); MDOC_IOS_TEST_WORKERS=<n> runs them on n clones of it
[macos]
ios-test device="iPhone 17": ios-core
    os=$(sed -n 's/^IOS_SIM_RUNTIME=//p' scripts/tools.env); skip=$(bash scripts/quarantine.sh ios) || exit 1; workers=${MDOC_IOS_TEST_WORKERS:-}; xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination "platform=iOS Simulator,OS=$os,name={{device}}" -derivedDataPath target/ios/DerivedData -quiet ${workers:+-parallel-testing-enabled YES -parallel-testing-worker-count "$workers"} test $skip

# The device UI tests on a connected iPhone: the review flow, then the long jobs or, below the floor, the below-floor test (ADR 0088)
[macos]
ios-device-test udid tier="auto": ios-core
    scripts/ios-device-test.sh "{{udid}}" "{{tier}}"

# Take the iOS UI survey (every screen, light and dark) into a folder (ADR 0087)
[macos]
ios-survey out device="iPhone 17 Pro": ios-core
    scripts/ios-survey.sh "{{device}}" "{{out}}"

# Regenerate the icon set from build/appicon.svg (macOS)
icon:
    scripts/make-icon.sh

# Lint the GitHub workflows
actionlint:
    actionlint

# Install the pinned tools from scripts/tools.env
setup:
    scripts/setup.sh

# Report missing or mismatched tools
doctor:
    scripts/doctor.sh

# Size of each worktree's target/ (ADR 0050)
disk:
    scripts/disk.sh

# Remove a retired worktree and its target/; refuses any other local files
retire-worktree path:
    scripts/retire-worktree.sh "{{path}}"
