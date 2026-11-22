#!/usr/bin/env bash
# Usage: scripts/ios/test-ios-test-part.sh
# scripts/ios/ios-test-part.sh without Xcode (#1065): a fake `just` stands
# in for the `ios-test` recipe's xcodebuild, failing or passing as each case
# plans and leaving an .xcresult whose tests a fake xcrun prints, the
# checked-in sample of a runner that failed to start
# (scripts/ios/fixtures/ios-runner-failed.json) or a failed test. A UI part
# whose runner failed, then passed, retries once without building on an
# erased Simulator and is green with a warning; one whose runner fails twice
# fails; a failed test, and the unit part, never retry. Skipped on Windows:
# the script runs only on macOS runners (ci-ios.yml). Bash 3.2 compatible.
source "$(dirname "$0")/../lib.sh"
if [ "$(platform)" = windows-x64 ]; then
  echo "skip: ios-test-part.sh runs only on macOS"
  exit 0
fi
root=$PWD
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# has <file> <line>: the file holds the line, whole.
has() { grep -qxF -- "$2" "$1" || fail "no line '$2' in $1: $(cat "$1")"; }

runner_json="$root/scripts/ios/fixtures/ios-runner-failed.json"
# The second way it fails to start (run 44940648698's attempt 2).
sed 's/The test runner crashed while preparing to run tests: .*"/The test runner failed to initialize for UI testing. (Underlying Error: Timed out waiting for AX loaded notification)"/' \
  "$runner_json" >"$tmp/runner-ax.json"
grep -q 'AX loaded' "$tmp/runner-ax.json" || fail "no AX sample"
# A test that started and failed.
printf '%s' '{"testNodes":[{"nodeType":"Test Plan","name":"mdoc","children":[{"nodeType":"UI test bundle","name":"mdocUITests","children":[{"nodeType":"Test Suite","name":"S","children":[{"nodeType":"Test Case","name":"testA()","nodeIdentifier":"S/testA()","result":"Failed","children":[{"nodeType":"Failure Message","name":"S.swift:9: failed"}]}]}]}]}]}' >"$tmp/test.json"

mkdir -p "$tmp/bin"
# just: logs `<MDOC_IOS_TEST_ACTION>|<args>` and does the next line of
# $JUST_PLAN: `pass`; `fail <tests.json>`, which leaves an .xcresult
# holding that JSON and exits 65, as xcodebuild's failed test does; or
# `exit <n>` alone, as a failed build does.
cat >"$tmp/bin/just" <<'EOF'
#!/usr/bin/env bash
echo "${MDOC_IOS_TEST_ACTION:-}|$*" >>"$JUST_LOG"
n=$(wc -l <"$JUST_LOG" | tr -d ' ')
step=$(sed -n "${n}p" "$JUST_PLAN")
case "$step" in
  pass) exit 0 ;;
  "exit "*) exit "${step#exit }" ;;
  "fail "*)
    result="target/ios/DerivedData/Logs/Test/Test-mdoc-$n.xcresult"
    mkdir -p "$result" && cp "${step#fail }" "$result/tests.json"
    exit 65
    ;;
  *) echo "fake just: no step $n" >&2 && exit 99 ;;
esac
EOF
# xcrun: xcresulttool prints the .xcresult's tests.json; simctl lists one
# booted device and logs what it is asked.
cat >"$tmp/bin/xcrun" <<'EOF'
#!/usr/bin/env bash
case "$1 $2" in
  "xcresulttool get") cat "$6/tests.json" ;;
  "simctl list")
    echo "== Devices =="
    echo "-- iOS 27.2 --"
    echo "    iPhone 17 (4AD2086B-7B8F-4608-A742-AA422A0B8D74) (Booted) "
    ;;
  *) echo "$*" >>"$XCRUN_LOG" ;;
esac
EOF
chmod +x "$tmp/bin/just" "$tmp/bin/xcrun"
export PATH="$tmp/bin:$PATH"

# run <case> <part> <plan step>...: runs the script in a fresh folder,
# its output in $out and its status in $rc.
run() {
  local name=$1 part=$2
  shift 2
  dir="$tmp/$name" && mkdir -p "$dir"
  printf '%s\n' "$@" >"$dir/plan"
  export JUST_PLAN="$dir/plan" JUST_LOG="$dir/just.log" XCRUN_LOG="$dir/xcrun.log" GITHUB_STEP_SUMMARY="$dir/summary"
  : >"$JUST_LOG" && : >"$XCRUN_LOG"
  out="$dir/out" rc=0
  (cd "$dir" && bash "$root/scripts/ios/ios-test-part.sh" "$part" mdocUITests/AUITests mdocUITests/BUITests) >"$out" 2>&1 || rc=$?
}
first='|--no-deps ios-app ios-test iPhone 17 mdocUITests/AUITests mdocUITests/BUITests'
again='test-without-building|--no-deps ios-test iPhone 17 mdocUITests/AUITests mdocUITests/BUITests'
warning='::warning::ci/ios (ui 1): UI test runner failed to start; retried once'

# A green part: one run, nothing more.
run green 'ui 1' pass
[ "$rc" = 0 ] || fail "a green part failed: $(cat "$out")"
[ "$(cat "$JUST_LOG")" = "$first" ] || fail "a green part ran more: $(cat "$JUST_LOG")"
[ -s "$out" ] && fail "a green part said something: $(cat "$out")"

# The runner fails to start, then passes: one retry, without building, of
# the same tests on the erased booted Simulator; green, with the warning;
# the failed attempt's .xcresult out of the later steps' folder.
run retried 'ui 1' "fail $runner_json" pass
[ "$rc" = 0 ] || fail "a pass on the retry failed the part: $(cat "$out")"
has "$JUST_LOG" "$first"
has "$JUST_LOG" "$again"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 2 ] || fail "not one retry: $(cat "$JUST_LOG")"
has "$XCRUN_LOG" 'simctl shutdown all'
has "$XCRUN_LOG" 'simctl erase 4AD2086B-7B8F-4608-A742-AA422A0B8D74'
has "$out" "$warning"
[ "$(grep -c '^::warning::' "$out")" = 1 ] || fail "not one warning: $(cat "$out")"
has "$GITHUB_STEP_SUMMARY" '### ci/ios (ui 1): UI test runner failed to start; retried once'
[ -d "$dir/target/ios/DerivedData/Logs/RunnerFailed/Test-mdoc-1.xcresult" ] || fail "the failed attempt's xcresult did not move"
[ -z "$(ls "$dir/target/ios/DerivedData/Logs/Test")" ] || fail "the failed attempt's xcresult is still read: $(ls "$dir/target/ios/DerivedData/Logs/Test")"

# The runner fails to start twice (the crash, then the AX time-out): one
# retry, then the part fails with the retry's status, no warning, and the
# retry's .xcresult for the Failing tests step.
run twice 'ui 1' "fail $runner_json" "fail $tmp/runner-ax.json"
[ "$rc" = 65 ] || fail "two runner failures exited $rc: $(cat "$out")"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 2 ] || fail "not one retry: $(cat "$JUST_LOG")"
has "$JUST_LOG" "$again"
grep -q '^::warning::' "$out" && fail "a failed retry warned: $(cat "$out")"
[ -s "$GITHUB_STEP_SUMMARY" ] && fail "a failed retry wrote the summary: $(cat "$GITHUB_STEP_SUMMARY")"
[ -d "$dir/target/ios/DerivedData/Logs/Test/Test-mdoc-2.xcresult" ] || fail "the retry's xcresult is missing"

# The AX time-out alone retries too.
run ax 'ui 2' "fail $tmp/runner-ax.json" pass
[ "$rc" = 0 ] || fail "the AX time-out did not retry: $(cat "$out")"
has "$out" '::warning::ci/ios (ui 2): UI test runner failed to start; retried once'

# A test failed: no retry; the part fails.
run test 'ui 1' "fail $tmp/test.json" pass
[ "$rc" = 65 ] || fail "a failed test exited $rc: $(cat "$out")"
[ "$(cat "$JUST_LOG")" = "$first" ] || fail "a failed test retried: $(cat "$JUST_LOG")"
[ -s "$XCRUN_LOG" ] && fail "a failed test reset the Simulator: $(cat "$XCRUN_LOG")"
grep -q '^::warning::' "$out" && fail "a failed test warned: $(cat "$out")"

# The unit part's runner fails to start: unit tests never retry.
run unit unit "fail $runner_json" pass
[ "$rc" = 65 ] || fail "the unit part exited $rc: $(cat "$out")"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 1 ] || fail "the unit part retried: $(cat "$JUST_LOG")"

# A build failure (no .xcresult): no retry.
run build 'ui 3' 'exit 65' pass
[ "$rc" = 65 ] || fail "a build failure exited $rc: $(cat "$out")"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 1 ] || fail "a build failure retried: $(cat "$JUST_LOG")"

# Usage.
rc=0
bash scripts/ios/ios-test-part.sh >"$tmp/usage" 2>&1 || rc=$?
[ "$rc" = 2 ] || fail "usage exited $rc"
grep -q '^usage: scripts/ios/ios-test-part.sh <part>' "$tmp/usage" || fail "no usage: $(cat "$tmp/usage")"
echo "test-ios-test-part: ok"
