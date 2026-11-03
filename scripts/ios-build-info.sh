#!/usr/bin/env bash
# Usage: scripts/ios-build-info.sh
# Writes target/ios/Build.xcconfig, which apps/ios/Config/Build.xcconfig
# includes: the build number is the number of commits up to HEAD (on a `main`
# commit, ADR 0091's count) and MDOC_CORE_COMMIT is HEAD, shown in Settings ›
# About (ADR 0097). Rewritten only when it changes, so Xcode rebuilds nothing
# for an unchanged commit. A shallow clone counts only the commits it has, so
# its number is wrong: a warning for test builds, an error when
# MDOC_REQUIRE_FULL_HISTORY=1 (a build for TestFlight).
source "$(dirname "$0")/lib.sh"

out=${MDOC_BUILD_XCCONFIG:-target/ios/Build.xcconfig}
commit=$(git rev-parse HEAD)
count=$(git rev-list --count HEAD)
[ "$count" -gt 0 ] || die "no commits to count"
if [ "$(git rev-parse --is-shallow-repository)" = true ]; then
  [ "${MDOC_REQUIRE_FULL_HISTORY:-}" != 1 ] ||
    die "shallow clone: the build number needs the full history (git fetch --unshallow, or fetch-depth: 0)"
  echo "warning: shallow clone; build $count is not the commit count on main" >&2
fi
text="// Written by scripts/ios-build-info.sh; do not edit.
CURRENT_PROJECT_VERSION = $count
MDOC_CORE_COMMIT = $commit"

mkdir -p "$(dirname "$out")"
if ! [ -f "$out" ] || [ "$(cat "$out")" != "$text" ]; then
  printf '%s\n' "$text" >"$out"
fi
echo "build $count, core commit $commit"
