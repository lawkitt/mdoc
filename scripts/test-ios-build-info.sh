#!/usr/bin/env bash
# Usage: scripts/test-ios-build-info.sh
# scripts/ios-build-info.sh on scratch repositories (ADR 0097): the build
# number is the commit count, the commit is HEAD, and a shallow clone warns,
# or fails when a TestFlight build requires the full history. No network.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# run <repo> [env...]: the script from inside a scratch repository, since
# lib.sh moves to the root of the repository the script sits in.
run() {
  local repo=$1
  shift
  mkdir -p "$repo/scripts"
  cp scripts/ios-build-info.sh scripts/lib.sh "$repo/scripts/"
  env "$@" bash "$repo/scripts/ios-build-info.sh"
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

g() { git -C "$tmp/full" -c user.name=t -c user.email=t@example.invalid "$@"; }
git init -q "$tmp/full"
for n in 1 2 3; do g commit -q --allow-empty -m "c$n"; done
head=$(g rev-parse HEAD)

# Full history: three commits, HEAD's hash, rewritten only on change.
run "$tmp/full" MDOC_BUILD_XCCONFIG="$tmp/full.xcconfig" >/dev/null || fail "full history failed"
grep -qx "CURRENT_PROJECT_VERSION = 3" "$tmp/full.xcconfig" || fail "count: $(cat "$tmp/full.xcconfig")"
grep -qx "MDOC_CORE_COMMIT = $head" "$tmp/full.xcconfig" || fail "commit: $(cat "$tmp/full.xcconfig")"
touch -t 202001010000 "$tmp/full.xcconfig"
run "$tmp/full" MDOC_BUILD_XCCONFIG="$tmp/full.xcconfig" >/dev/null
[ -z "$(find "$tmp/full.xcconfig" -newermt 2020-01-02)" ] || fail "an unchanged commit rewrote the file"
run "$tmp/full" MDOC_REQUIRE_FULL_HISTORY=1 MDOC_BUILD_XCCONFIG="$tmp/full.xcconfig" >/dev/null ||
  fail "full history failed when required"

# Shallow clone of the same repository: a warning, or an error when required.
git clone -q --depth 1 "file://$tmp/full" "$tmp/shallow"
run "$tmp/shallow" MDOC_BUILD_XCCONFIG="$tmp/shallow.xcconfig" 2>"$tmp/warn" >/dev/null ||
  fail "a shallow clone failed without MDOC_REQUIRE_FULL_HISTORY"
grep -q "warning: shallow clone" "$tmp/warn" || fail "no shallow warning: $(cat "$tmp/warn")"
if run "$tmp/shallow" MDOC_REQUIRE_FULL_HISTORY=1 MDOC_BUILD_XCCONFIG="$tmp/strict.xcconfig" >/dev/null 2>&1; then
  fail "a shallow clone passed with MDOC_REQUIRE_FULL_HISTORY=1"
fi
[ ! -e "$tmp/strict.xcconfig" ] || fail "a refused build still wrote its settings"

echo "ios-build-info: ok"
