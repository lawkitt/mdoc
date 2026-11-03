#!/usr/bin/env bash
# Usage: scripts/ios-build-info.sh
# Writes target/ios/Build.xcconfig, which apps/ios/Config/Build.xcconfig
# includes: the build number is the number of commits up to HEAD (on a `main`
# commit, ADR 0091's count) and MDOC_CORE_COMMIT is HEAD, shown in Settings ›
# About (ADR 0097). Rewritten only when it changes, so Xcode rebuilds nothing
# for an unchanged commit.
source "$(dirname "$0")/lib.sh"

out=target/ios/Build.xcconfig
commit=$(git rev-parse HEAD)
count=$(git rev-list --count HEAD)
[ "$count" -gt 0 ] || die "no commits to count"
text="// Written by scripts/ios-build-info.sh; do not edit.
CURRENT_PROJECT_VERSION = $count
MDOC_CORE_COMMIT = $commit"

mkdir -p target/ios
if ! [ -f "$out" ] || [ "$(cat "$out")" != "$text" ]; then
  printf '%s\n' "$text" >"$out"
fi
echo "build $count, core commit $commit"
