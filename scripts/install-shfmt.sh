#!/usr/bin/env bash
# Usage: scripts/install-shfmt.sh [bin-dir]
# Installs the pinned shfmt release into bin-dir (default: cargo's bin
# directory), verified against the SHA-256 digests below. shfmt publishes no
# checksum file: a new SHFMT_VERSION needs the release's digests here (`gh
# release view v<version> -R mvdan/sh --json assets`).
source "$(dirname "$0")/lib.sh"
load_tools

bin=${1:-${CARGO_HOME:-$HOME/.cargo}/bin}
if "$bin/shfmt" --version 2>/dev/null | grep -qxF "v$SHFMT_VERSION"; then
  echo "shfmt $SHFMT_VERSION already installed"
  exit 0
fi

case "$(platform)" in
  macos-arm64) asset=darwin_arm64 exe=shfmt ;;
  windows-x64) asset=windows_amd64.exe exe=shfmt.exe ;;
  Linux-x86_64) asset=linux_amd64 exe=shfmt ;;
  *) die "no shfmt release mapping for $(platform)" ;;
esac
case "$SHFMT_VERSION/$asset" in
  3.14.1/darwin_arm64) want=b7c872db63553ccffc7253aba3ed7d4885a27d83f1ba567b1138c6315a5847e5 ;;
  3.14.1/windows_amd64.exe) want=13629ce28442ca80b6b5a819f7574ab39e1c28c6e26734ca816c9714e04851df ;;
  3.14.1/linux_amd64) want=76e77641faa025814b77f153b29796b8e6fa2fca03e0c76a691608b86c7ea7bf ;;
  *) die "no SHA-256 for shfmt $SHFMT_VERSION ($asset) in $0" ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/$exe" \
  "https://github.com/mvdan/sh/releases/download/v${SHFMT_VERSION}/shfmt_v${SHFMT_VERSION}_${asset}"
if command -v sha256sum >/dev/null; then sum=(sha256sum); else sum=(shasum -a 256); fi
got=$("${sum[@]}" "$tmp/$exe" | awk '{ print $1 }')
[ "$want" = "$got" ] || die "shfmt checksum mismatch"

mkdir -p "$bin"
chmod +x "$tmp/$exe"
cp "$tmp/$exe" "$bin/"
echo "shfmt $SHFMT_VERSION installed to $bin"
