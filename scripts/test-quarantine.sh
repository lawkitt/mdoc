#!/usr/bin/env bash
# Usage: scripts/test-quarantine.sh
# The quarantine list's parser and expiry check (scripts/quarantine.sh,
# ADR 0095) on scratch files. Bash 3.2 compatible.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

q() { QUARANTINE_TODAY=2026-11-10 bash scripts/quarantine.sh "$@"; }

# No file and an empty file: nothing listed, nothing filtered, check passes.
q check "$tmp/missing" || fail "a missing list failed check"
[ -z "$(q filter "$tmp/missing")" ] || fail "a missing list printed a filter"
printf '# only comments\n\n' >"$tmp/empty"
q check "$tmp/empty" || fail "a comment-only list failed check"
[ -z "$(q filter "$tmp/empty")" ] || fail "a comment-only list printed a filter"

# Two entries, one with spaces inside its filter: both in one filterset.
cat >"$tmp/list" <<'LIST'
# comment
test(=workspace::tests::sidebar::opens)  #510  2026-11-12
package(mdoc) & test(/^quit_flow::/)   #511  2026-11-10
LIST
q check "$tmp/list" || fail "dates on or after today failed check"
[ "$(q filter "$tmp/list")" = "(test(=workspace::tests::sidebar::opens)) | (package(mdoc) & test(/^quit_flow::/))" ] ||
  fail "filter: $(q filter "$tmp/list")"
[ "$(q list "$tmp/list" | wc -l | tr -d ' ')" = 2 ] || fail "list: $(q list "$tmp/list")"

# Filters holding `|` (regex alternation, a union) stay whole.
cat >"$tmp/pipes" <<'LIST'
test(/foo|bar/)  #510  2026-11-12
test(=a) | test(=b)  #511  2026-11-12
LIST
q check "$tmp/pipes" || fail "filters with | failed check"
[ "$(q filter "$tmp/pipes")" = "(test(/foo|bar/)) | (test(=a) | test(=b))" ] ||
  fail "filter with |: $(q filter "$tmp/pipes")"
[ "$(q list "$tmp/pipes" | cut -f1 | head -n1)" = "test(/foo|bar/)" ] || fail "list with |: $(q list "$tmp/pipes")"

# A line past its date fails check and names the test.
printf 'test(=old)  #400  2026-11-09\n' >>"$tmp/list"
if q check "$tmp/list" 2>"$tmp/err"; then fail "an expired line passed check"; fi
grep -q 'test(=old) (#400) expired on 2026-11-09' "$tmp/err" || fail "expiry message: $(cat "$tmp/err")"

# A malformed line (one space between fields, no issue) fails check and filter.
printf 'test(=bad) #401 2026-12-01\n' >"$tmp/bad"
if q check "$tmp/bad" 2>/dev/null; then fail "a malformed line passed check"; fi
if q filter "$tmp/bad" >/dev/null 2>&1; then fail "a malformed line passed filter"; fi
printf 'test(=bad)  2026-12-01\n' >"$tmp/bad"
if q check "$tmp/bad" 2>/dev/null; then fail "a line without an issue passed check"; fi

# App UI tests: Android ones go to AndroidJUnitRunner's notClass, iOS ones to
# -skip-testing, and neither into the nextest filter.
cat >"$tmp/apps" <<'LIST'
test(=a)  #510  2026-11-12
android:io.github.lawkitt.mdoc.OriginalTest#aPdfShowsItsOriginalPage  #537  2026-11-12
android:io.github.lawkitt.mdoc.SetupTest#opens  #538  2026-11-12
ios:mdocUITests/ReviewFlowUITests/testOpenReviewShareRestore  #542  2026-11-12
LIST
q check "$tmp/apps" || fail "app UI test lines failed check"
[ "$(q filter "$tmp/apps")" = "(test(=a))" ] || fail "filter with app tests: $(q filter "$tmp/apps")"
[ "$(q android "$tmp/apps")" = "io.github.lawkitt.mdoc.OriginalTest#aPdfShowsItsOriginalPage,io.github.lawkitt.mdoc.SetupTest#opens" ] ||
  fail "android: $(q android "$tmp/apps")"
[ "$(q ios "$tmp/apps")" = "-skip-testing:mdocUITests/ReviewFlowUITests/testOpenReviewShareRestore" ] || fail "ios: $(q ios "$tmp/apps")"
[ -z "$(q android "$tmp/pipes")$(q ios "$tmp/pipes")" ] || fail "a list without app tests printed app arguments"
for bad in 'android:OriginalTest' 'ios:ReviewFlowUITests/testOpenReviewShareRestore' 'android:a b#c' 'ios:t/c/m()'; do
  printf '%s  #542  2026-11-12\n' "$bad" >"$tmp/bad"
  if q check "$tmp/bad" 2>/dev/null; then fail "a malformed app test passed check: $bad"; fi
done
# The Android and iOS jobs pass them on.
grep -q 'quarantine.sh android' scripts/android-check.sh || fail "android-check.sh does not read the quarantine list"
grep -q 'quarantine.sh ios' justfile || fail "just ios-test does not read the quarantine list"

# The repository's own list parses.
bash scripts/quarantine.sh list >/dev/null || fail ".config/quarantine.txt does not parse"

echo "quarantine: all checks passed"
