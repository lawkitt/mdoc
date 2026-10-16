# mdoc development commands. CI runs the same recipes (ADR 0037).
# Logic beyond a single command lives in scripts/*.sh.

set shell := ["bash", "-euo", "pipefail", "-c"]
# A bare `bash` on Windows can resolve to WSL; Git Bash sees the Windows toolchain.
set windows-shell := ["C:/Program Files/Git/bin/bash.exe", "-euo", "pipefail", "-c"]

# List recipes
default:
    @just --list --unsorted

# Format, licences/advisories, clippy and tests — what CI runs
check: fmt-check deny lint test

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

# Cargo.lock must match Cargo.toml (ADR 0053); then clippy with warnings as errors
lint:
    cargo metadata --locked --format-version 1 > /dev/null
    cargo clippy --workspace --all-targets -- -D warnings

# Workspace tests
test:
    cargo test --workspace

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

# Build this machine's installers into dist/
package:
    scripts/package.sh

# Release gate: OCR from the packaged app (signed .dmg / release mdoc.exe)
smoke-package:
    scripts/smoke-package.sh

# In-app update from an older signed .dmg to the latest release (ADR 0040)
smoke-update *args:
    scripts/smoke-update.sh {{args}}

# Release commit + tag for <version>, push after confirmation (--no-push to skip)
release version *flags:
    scripts/release.sh {{version}} {{flags}}

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
