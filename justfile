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

# Isolated release-script regressions; no network, builds or real releases
test-release:
    bash scripts/test-release.sh

# Local CI runner regressions; fake gh and just, no network or builds
test-local-ci:
    bash scripts/test-local-ci.sh

# Local CI runner (ADR 0082): run, once, install, start, stop, status, uninstall
local-ci *args:
    scripts/local-ci.sh {{args}}

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

# cargo check of mdoc-ffi for iOS and Android; the Mac's local CI runs it (ADR 0085)
mobile:
    scripts/mobile-check.sh

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

# Run the iOS unit, parity and UI tests on an iOS 26.1 Simulator device
[macos]
ios-test device="iPhone 17": ios-core
    xcodebuild -project apps/ios/mdoc.xcodeproj -scheme mdoc -destination 'platform=iOS Simulator,OS=26.1,name={{device}}' -derivedDataPath target/ios/DerivedData -quiet test

# The device UI tests on a connected iPhone: the review flow, then the long jobs or, below the floor, the below-floor test (ADR 0088)
[macos]
ios-device-test udid tier="auto": ios-core
    scripts/ios-device-test.sh "{{udid}}" "{{tier}}"

# Take the iOS UI survey (every screen, light and dark) into a folder (ADR 0087)
[macos]
ios-survey out device="iPhone 17": ios-core
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
