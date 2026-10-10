#!/usr/bin/env bash
# Usage: scripts/doctor.sh
# Reports whether this machine has the pinned tools from scripts/tools.env.
# Exits non-zero when anything is missing or at the wrong version.
source "$(dirname "$0")/lib.sh"
load_tools

problems=0
check() { # name, expected version, actual version line
  local name=$1 want=$2 got=$3
  if [ -z "$got" ]; then
    printf '  %-16s missing (want %s)\n' "$name" "$want"
    problems=$((problems + 1))
  elif [[ $got != *"$want"* ]]; then
    printf '  %-16s %s (want %s)\n' "$name" "$got" "$want"
    problems=$((problems + 1))
  else
    printf '  %-16s ok %s\n' "$name" "$want"
  fi
}
out() { "$@" 2>/dev/null | grep -m1 . || true; }

toolchain=$(awk -F'"' '/^channel/ { print $2 }' rust-toolchain.toml)
echo "platform: $(platform)"
check rustc "$toolchain" "$(out rustc --version)"
check just "$JUST_VERSION" "$(out just --version)"
check cargo-deny "$CARGO_DENY_VERSION" "$(out cargo deny --version)"
check cargo-packager "$CARGO_PACKAGER_VERSION" "$(out cargo packager --version)"
check actionlint "$ACTIONLINT_VERSION" "$(out actionlint --version)"
if [ "$(platform)" = windows-x64 ]; then
  check cargo-wix "$CARGO_WIX_VERSION" "$(out cargo wix --version)"
  check "WiX (candle)" "${WIX_VERSION}" "$(out candle -?)"
fi

if [ "$problems" -gt 0 ]; then
  echo "$problems problem(s); run \`just setup\`"
  exit 1
fi
echo "all tools present"
