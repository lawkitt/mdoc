#!/usr/bin/env bash
# Usage: scripts/test-ios-failed-tests.sh
# scripts/ios-failed-tests.py (ADR 0113) without Xcode: a fake xcrun prints
# the checked-in xcresulttool sample (scripts/fixtures/ios-failed-tests.json),
# whose failed cases a part names, as the PC words its status, in its error,
# summary and output; ci/ios names each part that did not pass, from those
# outputs; a part with no xcresult, an unreadable one or no failed case
# names none. Bash 3.2 compatible.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# has <file> <line>: the file holds the line, whole.
has() { grep -qxF -- "$2" "$1" || fail "no line '$2' in $1: $(cat "$1")"; }

mkdir -p "$tmp/bin" "$tmp/results/Test-mdoc-1.xcresult" "$tmp/results/Test-mdoc-2.xcresult" "$tmp/empty"
cat >"$tmp/bin/xcrun" <<'EOF'
#!/usr/bin/env bash
echo "$*" >>"$XCRUN_LOG"
[ -z "${XCRUN_FAIL:-}" ] || { echo "xcresulttool: not a result bundle" >&2; exit 1; }
cat "$XCRUN_JSON"
EOF
chmod +x "$tmp/bin/xcrun"
export PATH="$tmp/bin:$PATH" XCRUN_LOG="$tmp/xcrun.log" XCRUN_JSON="$PWD/scripts/fixtures/ios-failed-tests.json"
export GITHUB_STEP_SUMMARY="$tmp/summary" GITHUB_OUTPUT="$tmp/output"
run() { python3 scripts/ios-failed-tests.py "$@" >"$tmp/out" 2>&1; }

# The sample is ui 2's xcresult on run 43375543082 (two failed cases, a
# passed and a skipped suite), trimmed.
one=ios:mdocUITests/PanesRowUITests/testPlantedFailure
two=ios:mdocUITests/ReviewPagerUITests/testTheCardSwipesAsACarouselAndStillPullsTheBar

# A part: every failed case of every xcresult once, sorted, with its
# target; no passed or skipped case, and no suite.
run part 'ui 2' "$tmp/results" || fail "part exited non-zero: $(cat "$tmp/out")"
has "$tmp/out" "::error::Failed in ci/ios (ui 2): $one and 1 more"
has "$XCRUN_LOG" "xcresulttool get test-results tests --path $tmp/results/Test-mdoc-1.xcresult --compact"
has "$XCRUN_LOG" "xcresulttool get test-results tests --path $tmp/results/Test-mdoc-2.xcresult --compact"
has "$GITHUB_OUTPUT" "ui2_tests=$one $two"
has "$GITHUB_STEP_SUMMARY" '### ci/ios (ui 2)'
has "$GITHUB_STEP_SUMMARY" "- \`$one\`"
has "$GITHUB_STEP_SUMMARY" "- \`$two\`"
[ "$(grep -c '^- ' "$GITHUB_STEP_SUMMARY")" = 2 ] || fail "summary lists other tests: $(cat "$GITHUB_STEP_SUMMARY")"

# One failed case alone: no "and 0 more".
case_one='{"testNodes":[{"nodeType":"Test Plan","name":"mdoc","children":[{"nodeType":"Unit test bundle","name":"mdocTests","children":[{"nodeType":"Test Suite","name":"ATests","children":[{"nodeType":"Test Case","name":"testA()","nodeIdentifier":"ATests/testA()","result":"Failed"}]}]}]}]}'
echo "$case_one" >"$tmp/one.json"
XCRUN_JSON="$tmp/one.json" run part unit "$tmp/results"
has "$tmp/out" "::error::Failed in ci/ios (unit): ios:mdocTests/ATests/testA"
has "$GITHUB_OUTPUT" "unit_tests=ios:mdocTests/ATests/testA"

# Names none, but still annotates the part: no xcresult (a build failure),
# an unreadable one (warned), or no failed case (the Release build failed).
: >"$GITHUB_OUTPUT"
run part unit "$tmp/empty"
has "$tmp/out" '::error::Failed in ci/ios (unit)'
has "$GITHUB_OUTPUT" 'unit_tests='
XCRUN_FAIL=1 run part unit "$tmp/results"
has "$tmp/out" '::error::Failed in ci/ios (unit)'
grep -q '^::warning::could not read Test-mdoc-2.xcresult: xcresulttool: not a result bundle' "$tmp/out" || fail "no warning: $(cat "$tmp/out")"
echo '{"testNodes":[]}' >"$tmp/none.json"
XCRUN_JSON="$tmp/none.json" run part unit "$tmp/results"
has "$tmp/out" '::error::Failed in ci/ios (unit)'
echo '{"devices":[]}' >"$tmp/bad.json"
XCRUN_JSON="$tmp/bad.json" run part unit "$tmp/results"
has "$tmp/out" '::error::Failed in ci/ios (unit)'

# A workflow command's data is escaped.
printf '%s' "${case_one/ATests\/testA()/ATests\/test%A()}" >"$tmp/pct.json"
XCRUN_JSON="$tmp/pct.json" run part unit "$tmp/results"
has "$tmp/out" '::error::Failed in ci/ios (unit): ios:mdocTests/ATests/test%25A'

# ci/ios: green says nothing; otherwise each part that did not pass, from
# needs.ios.outputs: its tests, "Failed in" alone, its status, or "did not
# finish" when it wrote none.
: >"$GITHUB_STEP_SUMMARY"
IOS_OUTPUTS='{"unit":"success","ui1":"success","ui2":"success","unit_tests":"","ui1_tests":"","ui2_tests":""}' run summary success unit 'ui 1' 'ui 2' || fail "green summary failed: $(cat "$tmp/out")"
if [ -s "$tmp/out" ] || [ -s "$GITHUB_STEP_SUMMARY" ]; then fail "green summary reported: $(cat "$tmp/out")"; fi
if IOS_OUTPUTS="{\"unit\":\"failure\",\"ui1\":\"success\",\"ui2\":\"failure\",\"unit_tests\":\"\",\"ui1_tests\":\"\",\"ui2_tests\":\"$one $two\"}" run summary failure unit 'ui 1' 'ui 2'; then fail 'red summary passed'; fi
has "$tmp/out" '::error::Failed in ci/ios (unit)'
has "$tmp/out" "::error::Failed in ci/ios (ui 2): $one and 1 more"
[ "$(grep -c '^::error::' "$tmp/out")" = 2 ] || fail "summary annotated a part that passed: $(cat "$tmp/out")"
has "$GITHUB_STEP_SUMMARY" "- \`$two\`"
if IOS_OUTPUTS='{"unit":"success","ui1":"cancelled","ui2":""}' run summary failure unit 'ui 1' 'ui 2'; then fail 'cancelled summary passed'; fi
has "$tmp/out" '::error::ci/ios (ui 1): cancelled'
has "$tmp/out" '::error::ci/ios (ui 2): did not finish'
if IOS_OUTPUTS='{"unit":"success","ui1":"success","ui2":"success"}' run summary cancelled unit 'ui 1' 'ui 2'; then fail 'cancelled run passed'; fi
has "$tmp/out" '::error::ci/ios: parts cancelled'
if IOS_OUTPUTS='' run summary skipped unit; then fail 'skipped run passed'; fi
has "$tmp/out" '::error::ci/ios (unit): did not finish'

# Usage.
if run nonsense; then fail 'a bad command passed'; fi
grep -q '^Usage: scripts/ios-failed-tests.py part' "$tmp/out" || fail "no usage: $(cat "$tmp/out")"
echo "test-ios-failed-tests: ok"
