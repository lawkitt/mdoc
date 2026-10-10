#!/usr/bin/env bash
# Usage: scripts/install-actionlint.sh [bin-dir]
# Installs the pinned actionlint release, verified against the release's
# checksum file, into bin-dir (default: cargo's bin directory).
source "$(dirname "$0")/lib.sh"
load_tools

bin=${1:-${CARGO_HOME:-$HOME/.cargo}/bin}
if "$bin/actionlint" --version 2>/dev/null | head -1 | grep -qxF "$ACTIONLINT_VERSION"; then
  echo "actionlint $ACTIONLINT_VERSION already installed"
  exit 0
fi

case "$(platform)" in
  macos-arm64) asset="actionlint_${ACTIONLINT_VERSION}_darwin_arm64.tar.gz" ;;
  windows-x64) asset="actionlint_${ACTIONLINT_VERSION}_windows_amd64.zip" ;;
  Linux-x86_64) asset="actionlint_${ACTIONLINT_VERSION}_linux_amd64.tar.gz" ;;
  *) die "no actionlint release mapping for $(platform)" ;;
esac

base="https://github.com/rhysd/actionlint/releases/download/v${ACTIONLINT_VERSION}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/$asset" "$base/$asset"
curl -fsSL -o "$tmp/sums.txt" "$base/actionlint_${ACTIONLINT_VERSION}_checksums.txt"
want=$(awk -v a="$asset" '$2 == a { print $1 }' "$tmp/sums.txt")
if command -v sha256sum >/dev/null; then sum=(sha256sum); else sum=(shasum -a 256); fi
got=$("${sum[@]}" "$tmp/$asset" | awk '{ print $1 }')
[ -n "$want" ] && [ "$want" = "$got" ] || die "actionlint checksum mismatch"

mkdir -p "$tmp/x" "$bin"
case "$asset" in
  *.zip) unzip -oq "$tmp/$asset" -d "$tmp/x" ;;
  *) tar -xzf "$tmp/$asset" -C "$tmp/x" ;;
esac
cp "$tmp/x/actionlint"* "$bin/"
echo "actionlint $ACTIONLINT_VERSION installed to $bin"
