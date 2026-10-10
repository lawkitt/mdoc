# Shared helpers for scripts/*.sh. Sourced, never executed.
# shellcheck shell=bash

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

die() {
  echo "error: $*" >&2
  exit 1
}

# The [package] version in Cargo.toml — the single source of truth (ADR 0037).
version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[package]") }
    in_pkg && /^version = / { gsub(/"/, "", $3); print $3; exit }
  ' Cargo.toml
}

is_semver() {
  [[ $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$ ]]
}

# Shipped targets (ADR 0037): macos-arm64 and windows-x64; anything else is
# reported as-is so callers can refuse it.
platform() {
  local os arch
  os=$(uname -s)
  arch=$(uname -m)
  case "$os/$arch" in
    Darwin/arm64) echo macos-arm64 ;;
    MINGW*/x86_64 | MSYS*/x86_64 | CYGWIN*/x86_64) echo windows-x64 ;;
    *) echo "$os-$arch" ;;
  esac
}

require_shipped_target() {
  case "$(platform)" in
    macos-arm64 | windows-x64) ;;
    *) die "$1 runs only on Apple Silicon macOS or Windows x64 (this is $(platform))" ;;
  esac
}

# y/N prompt; true only for y/Y. Strips the carriage return a Windows console
# appends, which would otherwise make every answer "no".
confirm() {
  local answer
  read -r -p "$1 [y/N] " answer || return 1
  answer=${answer%$'\r'}
  [[ $answer =~ ^[Yy]$ ]]
}

# WiX 3's candle.exe: on PATH, or under %WIX%. The WiX 3 installer sets only
# WIX (which cargo-wix reads), not PATH.
candle() {
  if type -P candle >/dev/null; then
    command candle "$@"
  elif [ -n "${WIX:-}" ] && [ -x "$(cygpath -u "$WIX")/bin/candle.exe" ]; then
    "$(cygpath -u "$WIX")/bin/candle.exe" "$@"
  else
    return 127
  fi
}

# Loads scripts/tools.env into the environment.
load_tools() {
  set -a
  # shellcheck source=tools.env
  source scripts/tools.env
  set +a
}
