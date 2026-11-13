#!/usr/bin/env bash
# Usage: scripts/ios-ui-shards.sh <shard> | --check
# The UI test classes (mdocUITests) split over ci-ios.yml's UI parts,
# `ci/ios (ui 1)` and `ci/ios (ui 2)` (ADR 0112): `<shard>` prints that
# shard's classes as xcodebuild `-only-testing` identifiers on one line
# (`mdocUITests/EditToolbarUITests ...`), which `just ios-test` takes as its
# `only` arguments. `--check` prints nothing. Either way it first fails,
# naming the class, when a test class under apps/ios/mdocUITests is in no
# shard or in more than one, or a listed class is not there: so a new UI
# test class fails `lint (ubuntu)` until it is added below, and never runs
# in no shard or twice. A test class is a Swift class there that inherits
# XCTestCase or whose name ends in `Tests`.
#
# Balanced by each class's time on hosted macos-26 (the xcresult of run
# 43251870301, after the UI test sweep #829), in seconds beside it; skipped
# tests (survey, explore, device, audit, the timing tests outside the
# nightly) cost about a second each. A new class goes to the shard with the
# smaller total. The timing tests (LivePreviewUITests, `timing: true`) stay
# in one shard, about 4 minutes more there each night. Quarantined tests
# (.config/quarantine.txt) are not counted: all of ReviewFlowUITests' and
# ReviewToolsUITests' are, so they sit one in each shard. Bash 3.2
# compatible.
source "$(dirname "$0")/lib.sh"

shards='
1 EditToolbarUITests 208
1 LivePreviewUITests 157
1 IdentitiesGesturesUITests 135
1 NewDocumentUITests 67
1 ReviewFlowUITests 0
1 DeviceLongJobsUITests 3
2 TableEditingUITests 202
2 EditModeUITests 146
2 ReviewPagerUITests 95
2 PanesRowUITests 63
2 SidebarSearchUITests 42
2 ReviewToolsUITests 0
2 AccessibilityAuditExploreUITests 7
2 DeviceTapTimingUITests 6
2 SurveyUITests 2
2 DeviceBelowFloorUITests 2
2 RenderedMarkdownUITests 0
2 LargeTextExploreUITests 0
2 OCRResumeUITests 0
'
count=2
dir=${MDOC_UI_TESTS_DIR:-apps/ios/mdocUITests}

case "${1:-}" in
  --check) want= ;;
  [1-9])
    [ "$1" -le "$count" ] || die "no shard $1: there are $count"
    want=$1
    ;;
  *) die "usage: scripts/ios-ui-shards.sh <shard 1-$count> | --check" ;;
esac
[ "$#" -eq 1 ] || die "usage: scripts/ios-ui-shards.sh <shard 1-$count> | --check"
[ -d "$dir" ] || die "no $dir"

# The test classes declared under the folder, one per line.
found=$(find "$dir" -type f -name '*.swift' -print0 | xargs -0 awk '
  {
    line = $0
    sub(/\/\/.*/, "", line)
    if (!match(line, /(^|[^A-Za-z0-9_])class[ \t]+[A-Za-z_][A-Za-z0-9_]*/)) next
    decl = substr(line, RSTART, RLENGTH)
    sub(/^[^A-Za-z0-9_]?class[ \t]+/, "", decl)
    rest = substr(line, RSTART + RLENGTH)
    if (rest ~ /^[ \t]*:[^{]*XCTestCase/ || decl ~ /Tests$/) print decl
  }' | LC_ALL=C sort)

listed=$(printf '%s\n' "$shards" | awk 'NF { print $2 }' | LC_ALL=C sort)
bad=0
for c in $(printf '%s\n' "$listed" | uniq -d); do
  echo "error: $c is in more than one shard (scripts/ios-ui-shards.sh)" >&2
  bad=1
done
for c in $(printf '%s\n' "$found" | uniq -d); do
  echo "error: $c is declared more than once under $dir" >&2
  bad=1
done
for c in $(LC_ALL=C comm -23 <(printf '%s\n' "$found" | uniq) <(printf '%s\n' "$listed" | uniq)); do
  echo "error: $c (under $dir) is in no shard: add it to the shard with the smaller total in scripts/ios-ui-shards.sh" >&2
  bad=1
done
for c in $(LC_ALL=C comm -13 <(printf '%s\n' "$found" | uniq) <(printf '%s\n' "$listed" | uniq)); do
  echo "error: $c is in a shard but not under $dir: remove it from scripts/ios-ui-shards.sh" >&2
  bad=1
done
for s in $(printf '%s\n' "$shards" | awk 'NF { print $1 }' | LC_ALL=C sort -u); do
  case "$s" in [1-9]) [ "$s" -le "$count" ] && continue ;; esac
  echo "error: shard $s in scripts/ios-ui-shards.sh: shards are 1 to $count" >&2
  bad=1
done
[ "$bad" = 0 ] || exit 1

[ -n "$want" ] || exit 0
only=$(printf '%s\n' "$shards" | awk -v s="$want" '$1 == s { printf "%smdocUITests/%s", (n++ ? " " : ""), $2 }')
[ -n "$only" ] || die "shard $want has no classes"
echo "$only"
