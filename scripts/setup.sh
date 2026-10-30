#!/usr/bin/env bash
# Usage: scripts/setup.sh
# Installs the tools pinned in scripts/tools.env for this machine: the Rust
# toolchain, Rust tools through cargo-binstall (or cargo install), actionlint
# and shfmt (scripts/install-actionlint.sh, scripts/install-shfmt.sh).
source "$(dirname "$0")/lib.sh"
load_tools

rustup toolchain install # reads rust-toolchain.toml

crate() { # crate, version, probe command...
  local name=$1 want=$2
  shift 2
  if "$@" 2>/dev/null | head -1 | grep -qF "$want"; then
    echo "$name $want already installed"
  elif command -v cargo-binstall >/dev/null; then
    cargo binstall --no-confirm --locked "$name@$want"
  else
    cargo install --locked "$name@$want"
  fi
}

crate just "$JUST_VERSION" just --version
crate cargo-deny "$CARGO_DENY_VERSION" cargo deny --version
crate cargo-nextest "$CARGO_NEXTEST_VERSION" cargo nextest --version
crate cargo-packager "$CARGO_PACKAGER_VERSION" cargo packager --version

scripts/install-actionlint.sh
scripts/install-shfmt.sh

scripts/doctor.sh || true
