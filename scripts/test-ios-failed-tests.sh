#!/usr/bin/env bash
# Usage: scripts/test-ios-failed-tests.sh
# scripts/ios-failed-tests.py (ADR 0113) without Xcode: a fake xcrun prints
# the checked-in xcresulttool sample (scripts/fixtures/ios-failed-tests.json),
# whose failed cases a part names, as the PC words its status, in its error,
# summary and output; a case that passed on its retry, warned in a green part
# or a red one and never a failure; ci/ios names each part that did not pass,
# and every retried case, from those outputs; a part with no xcresult, an
# unreadable one or no failed case names none. Bash 3.2 compatible.
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
# passed and a skipped suite), trimmed, with run 43392172167's case that
# failed and passed on its retry.
one=ios:mdocUITests/PanesRowUITests/testPlantedFailure
two=ios:mdocUITests/ReviewPagerUITests/testTheCardSwipesAsACarouselAndStillPullsTheBar
again=ios:mdocUITests/PanesRowUITests/testPlantedFailOnce

# A part: every failed case of every xcresult once, sorted, with its
# target; no passed or skipped case, and no suite.
run part 'ui 2' failure "$tmp/results" || fail "part exited non-zero: $(cat "$tmp/out")"
has "$tmp/out" "::error::Failed in ci/ios (ui 2): $one and 1 more"
has "$XCRUN_LOG" "xcresulttool get test-results tests --path $tmp/results/Test-mdoc-1.xcresult --compact"
has "$XCRUN_LOG" "xcresulttool get test-results tests --path $tmp/results/Test-mdoc-2.xcresult --compact"
has "$GITHUB_OUTPUT" "ui2_tests=$one $two"
has "$GITHUB_OUTPUT" "ui2_retried=$again"
has "$GITHUB_OUTPUT" "retried=1"
has "$tmp/out" "::warning::Passed on retry in ci/ios (ui 2): $again"
has "$GITHUB_STEP_SUMMARY" '### ci/ios (ui 2)'
has "$GITHUB_STEP_SUMMARY" "- \`$one\`"
has "$GITHUB_STEP_SUMMARY" "- \`$two\`"
has "$GITHUB_STEP_SUMMARY" '### ci/ios (ui 2): passed on retry'
has "$GITHUB_STEP_SUMMARY" "- \`$again\`"
[ "$(grep -c '^- ' "$GITHUB_STEP_SUMMARY")" = 3 ] || fail "summary lists other tests: $(cat "$GITHUB_STEP_SUMMARY")"

# One failed case alone: no "and 0 more".
case_one='{"testNodes":[{"nodeType":"Test Plan","name":"mdoc","children":[{"nodeType":"Unit test bundle","name":"mdocTests","children":[{"nodeType":"Test Suite","name":"ATests","children":[{"nodeType":"Test Case","name":"testA()","nodeIdentifier":"ATests/testA()","result":"Failed"}]}]}]}]}'
echo "$case_one" >"$tmp/one.json"
XCRUN_JSON="$tmp/one.json" run part unit failure "$tmp/results"
has "$tmp/out" "::error::Failed in ci/ios (unit): ios:mdocTests/ATests/testA"
has "$GITHUB_OUTPUT" "unit_tests=ios:mdocTests/ATests/testA"

# Names none, but still annotates the part: no xcresult (a build failure),
# an unreadable one (warned), or no failed case (the Release build failed).
: >"$GITHUB_OUTPUT"
run part unit failure "$tmp/empty"
has "$tmp/out" '::error::Failed in ci/ios (unit)'
has "$GITHUB_OUTPUT" 'unit_tests='
XCRUN_FAIL=1 run part unit failure "$tmp/results"
has "$tmp/out" '::error::Failed in ci/ios (unit)'
grep -q '^::warning::could not read Test-mdoc-2.xcresult: xcresulttool: not a result bundle' "$tmp/out" || fail "no warning: $(cat "$tmp/out")"
echo '{"testNodes":[]}' >"$tmp/none.json"
XCRUN_JSON="$tmp/none.json" run part unit failure "$tmp/results"
has "$tmp/out" '::error::Failed in ci/ios (unit)'
echo '{"devices":[]}' >"$tmp/bad.json"
XCRUN_JSON="$tmp/bad.json" run part unit failure "$tmp/results"
has "$tmp/out" '::error::Failed in ci/ios (unit)'

# A workflow command's data is escaped.
printf '%s' "${case_one/ATests\/testA()/ATests\/test%A()}" >"$tmp/pct.json"
XCRUN_JSON="$tmp/pct.json" run part unit failure "$tmp/results"
has "$tmp/out" '::error::Failed in ci/ios (unit): ios:mdocTests/ATests/test%25A'

# A case that failed and passed on its retry (a Failed and a Passed
# repetition, under the case or its device): a warning each in a green part,
# with no error, its own summary section and outputs; in a red part beside
# the failures, never one of them. A case whose repetitions all failed, or
# retried on one device and failed on another, is a failure.
rp() { # result repetition-results...
  local r out=''
  for r in "${@:2}"; do out="$out${out:+,}{\"nodeType\":\"Repetition\",\"name\":\"Retry\",\"result\":\"$r\"}"; done
  printf '{"nodeType":"Test Case","name":"%s()","nodeIdentifier":"%s()","result":"%s","children":[%s]}' "$NAME" "$NAME" "$1" "$out"
}
retry_tree() { # test cases...
  local IFS=,
  printf '{"testNodes":[{"nodeType":"Test Plan","name":"mdoc","children":[{"nodeType":"UI test bundle","name":"mdocUITests","children":[{"nodeType":"Test Suite","name":"S","children":[%s]}]}]}]}' "$*"
}
retry_tree "$(NAME=S/testB rp Passed Failed Passed)" "$(NAME=S/testA rp Failed Failed Passed)" "$(NAME=S/testOk rp Passed)" \
  "$(NAME=S/testBad rp Failed Failed Failed)" >"$tmp/retried.json"
: >"$GITHUB_STEP_SUMMARY" && : >"$GITHUB_OUTPUT"
mkdir -p "$tmp/one-result/Test-mdoc-1.xcresult"
XCRUN_JSON="$tmp/retried.json" run part 'ui 1' success "$tmp/one-result" || fail "green part exited non-zero: $(cat "$tmp/out")"
has "$tmp/out" '::warning::Passed on retry in ci/ios (ui 1): ios:mdocUITests/S/testA'
has "$tmp/out" '::warning::Passed on retry in ci/ios (ui 1): ios:mdocUITests/S/testB'
[ "$(grep -c . "$tmp/out")" = 2 ] || fail "a green part says more than its retried tests: $(cat "$tmp/out")"
has "$GITHUB_OUTPUT" 'ui1_retried=ios:mdocUITests/S/testA ios:mdocUITests/S/testB'
has "$GITHUB_OUTPUT" 'ui1_tests=ios:mdocUITests/S/testBad'
has "$GITHUB_OUTPUT" 'retried=2'
has "$GITHUB_STEP_SUMMARY" '### ci/ios (ui 1): passed on retry'
has "$GITHUB_STEP_SUMMARY" "- \`ios:mdocUITests/S/testA\`"
grep -q 'testBad\|testOk' "$GITHUB_STEP_SUMMARY" && fail "the summary lists a failure or a pass as retried: $(cat "$GITHUB_STEP_SUMMARY")"
XCRUN_JSON="$tmp/retried.json" run part 'ui 1' failure "$tmp/one-result"
has "$tmp/out" '::error::Failed in ci/ios (ui 1): ios:mdocUITests/S/testBad'
has "$tmp/out" '::warning::Passed on retry in ci/ios (ui 1): ios:mdocUITests/S/testB'
# Under a device node, and Xcode wording the case itself Failed: retried.
retry_tree "{\"nodeType\":\"Device\",\"name\":\"iPhone 17\",\"children\":[$(NAME=S/testD rp Failed Failed Passed)]}" >"$tmp/device.json"
XCRUN_JSON="$tmp/device.json" run part 'ui 2' success "$tmp/one-result"
has "$tmp/out" '::warning::Passed on retry in ci/ios (ui 2): ios:mdocUITests/S/testD'
# Retried on one device, failed on another: a failure.
retry_tree "$(NAME=S/testE rp Passed Failed Passed)" "$(NAME=S/testE rp Failed Failed Failed)" >"$tmp/split.json"
: >"$GITHUB_OUTPUT"
XCRUN_JSON="$tmp/split.json" run part 'ui 2' failure "$tmp/one-result"
has "$tmp/out" '::error::Failed in ci/ios (ui 2): ios:mdocUITests/S/testE'
has "$GITHUB_OUTPUT" 'ui2_retried='
grep -q '^::warning::' "$tmp/out" && fail "a failure warned as retried: $(cat "$tmp/out")"
# A green part with nothing retried says nothing.
XCRUN_JSON="$tmp/none.json" run part 'ui 2' success "$tmp/one-result"
[ -s "$tmp/out" ] && fail "a clean green part reported: $(cat "$tmp/out")"

# ci/ios: green says nothing; otherwise each part that did not pass, from
# needs.ios.outputs: its tests, "Failed in" alone, its status, or "did not
# finish" when it wrote none.
: >"$GITHUB_STEP_SUMMARY"
IOS_OUTPUTS='{"unit":"success","ui1":"success","ui2":"success","unit_tests":"","ui1_tests":"","ui2_tests":""}' run summary success unit 'ui 1' 'ui 2' || fail "green summary failed: $(cat "$tmp/out")"
if [ -s "$tmp/out" ] || [ -s "$GITHUB_STEP_SUMMARY" ]; then fail "green summary reported: $(cat "$tmp/out")"; fi
# ... but for its retried tests, green or not, each part's.
IOS_OUTPUTS='{"unit":"success","ui1":"success","ui2":"success","ui1_retried":"ios:mdocUITests/S/testA","ui2_retried":"ios:mdocUITests/S/testB ios:mdocUITests/S/testC"}' run summary success unit 'ui 1' 'ui 2' ||
  fail "green summary with retried tests failed: $(cat "$tmp/out")"
has "$tmp/out" '::warning::Passed on retry in ci/ios (ui 1): ios:mdocUITests/S/testA'
has "$tmp/out" '::warning::Passed on retry in ci/ios (ui 2): ios:mdocUITests/S/testB'
has "$tmp/out" '::warning::Passed on retry in ci/ios (ui 2): ios:mdocUITests/S/testC'
[ "$(grep -c . "$tmp/out")" = 3 ] || fail "green summary says more than its retried tests: $(cat "$tmp/out")"
has "$GITHUB_STEP_SUMMARY" '### ci/ios (ui 2): passed on retry'
: >"$GITHUB_STEP_SUMMARY"
if IOS_OUTPUTS='{"unit":"success","ui1":"failure","ui2":"success","ui1_tests":"ios:mdocUITests/S/testX","ui2_retried":"ios:mdocUITests/S/testB"}' run summary failure unit 'ui 1' 'ui 2'; then
  fail 'red summary with a retried test passed'
fi
has "$tmp/out" '::warning::Passed on retry in ci/ios (ui 2): ios:mdocUITests/S/testB'
has "$tmp/out" '::error::Failed in ci/ios (ui 1): ios:mdocUITests/S/testX'
: >"$GITHUB_STEP_SUMMARY"
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
