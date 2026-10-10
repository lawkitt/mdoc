#!/usr/bin/env bash
# Usage: scripts/setup.sh
# Installs the tools pinned in scripts/tools.env for this machine: the Rust
# toolchain, Rust tools through cargo-binstall (or cargo install), actionlint
# (scripts/install-actionlint.sh), and on Windows the WiX Toolset after
# confirmation.
source "$(dirname "$0")/lib.sh"
load_tools

rustup toolchain install   # reads rust-toolchain.toml

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
crate cargo-packager "$CARGO_PACKAGER_VERSION" cargo packager --version

scripts/install-actionlint.sh

if [ "$(platform)" = windows-x64 ]; then
  crate cargo-wix "$CARGO_WIX_VERSION" cargo wix --version
  if ! candle -? >/dev/null 2>&1; then
    if confirm "Install the WiX Toolset $WIX_VERSION with winget (needed for .msi)?"; then
      winget.exe install --exact --id WiXToolset.WiXToolset --version "$WIX_VERSION"
      echo "open a new terminal, then run \`just doctor\`: WiX sets WIX (not PATH) for new sessions"
    fi
  fi
fi

scripts/doctor.sh || true
