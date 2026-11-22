#!/usr/bin/env bash
# Usage: scripts/ios/test-ios-test-part.sh
# scripts/ios/ios-test-part.sh without Xcode (#1065): a fake `just` stands
# in for the `ios-test` recipe's xcodebuild, failing or passing as each case
# plans and leaving an .xcresult whose tests a fake xcrun prints, the
# checked-in sample of a runner that failed to start
# (scripts/ios/fixtures/ios-runner-failed.json) or a failed test. Every part
# tests without building (the core job built the app, #1076). A UI part
# whose runner failed, then passed, retries once on its erased device and is
# green with a warning; one whose runner fails twice fails; a failed test,
# the unit part, and a device that cannot be found or erased never retry. The `ios-test` recipe's own line, on a fake
# xcodebuild, runs the action MDOC_IOS_TEST_ACTION names. Skipped on Windows:
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
# xcrun: xcresulttool prints the .xcresult's tests.json; simctl lists an
# iPhone 17 in each pinned runtime (scripts/tools.env) and an iPhone 17 Pro
# (no iPhone 17 in the nightly's with XCRUN_NO_DEVICE), and logs what else
# it is asked, failing an erase with XCRUN_ERASE_FAIL.
cat >"$tmp/bin/xcrun" <<'EOF'
#!/usr/bin/env bash
case "$1 $2" in
  "xcresulttool get") cat "$6/tests.json" ;;
  "simctl list")
    echo "== Devices =="
    echo "-- iOS $IOS_SIM_RUNTIME --"
    echo "    iPhone 17 (11111111-2222-3333-4444-555555555555) (Shutdown) "
    echo "-- iOS $IOS_SIM_BUILDS_RUNTIME --"
    echo "    iPhone 17 Pro (99999999-2222-3333-4444-555555555555) (Shutdown) "
    [ -n "${XCRUN_NO_DEVICE:-}" ] || echo "    iPhone 17 (4AD2086B-7B8F-4608-A742-AA422A0B8D74) (Booted) "
    ;;
  "simctl erase")
    echo "$*" >>"$XCRUN_LOG"
    [ -z "${XCRUN_ERASE_FAIL:-}" ] || exit 1
    ;;
  *) echo "$*" >>"$XCRUN_LOG" ;;
esac
EOF
# xcodebuild: logs its arguments.
cat >"$tmp/bin/xcodebuild" <<'EOF'
#!/usr/bin/env bash
echo "$*" >>"$XCODEBUILD_LOG"
EOF
chmod +x "$tmp/bin/just" "$tmp/bin/xcrun" "$tmp/bin/xcodebuild"
# The nightly's pin, whose runtime is the second section's.
export PATH="$tmp/bin:$PATH" MDOC_XCODE=builds
IOS_SIM_RUNTIME=$(sed -n 's/^IOS_SIM_RUNTIME=//p' scripts/tools.env)
IOS_SIM_BUILDS_RUNTIME=$(sed -n 's/^IOS_SIM_BUILDS_RUNTIME=//p' scripts/tools.env)
export IOS_SIM_RUNTIME IOS_SIM_BUILDS_RUNTIME
if [ -z "$IOS_SIM_RUNTIME" ] || [ -z "$IOS_SIM_BUILDS_RUNTIME" ]; then fail "no runtimes in scripts/tools.env"; fi

# run <case> <part> <plan step>...: runs the script in a fresh folder that
# links scripts/, its output in $out and its status in $rc.
run() {
  local name=$1 part=$2
  shift 2
  dir="$tmp/$name" && mkdir -p "$dir" && ln -s "$root/scripts" "$dir/scripts"
  printf '%s\n' "$@" >"$dir/plan"
  export JUST_PLAN="$dir/plan" JUST_LOG="$dir/just.log" XCRUN_LOG="$dir/xcrun.log" GITHUB_STEP_SUMMARY="$dir/summary"
  : >"$JUST_LOG" && : >"$XCRUN_LOG"
  out="$dir/out" rc=0
  (cd "$dir" && bash "$root/scripts/ios/ios-test-part.sh" "$part" mdocUITests/AUITests mdocUITests/BUITests) >"$out" 2>&1 || rc=$?
}
# Every run, the first and the retry, tests the core job's app; ci-ios.yml
# reads the line that says so to skip the core's download in a UI part.
grep -qx 'export MDOC_IOS_TEST_ACTION=test-without-building' scripts/ios/ios-test-part.sh ||
  fail "ios-test-part.sh lost the line ci-ios.yml looks for"
first='test-without-building|--no-deps ios-test iPhone 17 mdocUITests/AUITests mdocUITests/BUITests'
warning='::warning::ci/ios (ui 1): UI test runner failed to start; retried once'

# A green part: one run, nothing more.
run green 'ui 1' pass
[ "$rc" = 0 ] || fail "a green part failed: $(cat "$out")"
[ "$(cat "$JUST_LOG")" = "$first" ] || fail "a green part ran more: $(cat "$JUST_LOG")"
[ -s "$out" ] && fail "a green part said something: $(cat "$out")"

# The runner fails to start, then passes: one retry, without building, of
# the same tests on the erased iPhone 17 of the pin's runtime (not another
# runtime's, nor an iPhone 17 Pro); green, with the warning; the failed
# attempt's .xcresult out of the later steps' folder.
run retried 'ui 1' "fail $runner_json" pass
[ "$rc" = 0 ] || fail "a pass on the retry failed the part: $(cat "$out")"
[ "$(sort -u "$JUST_LOG")" = "$first" ] || fail "the retry ran something else: $(cat "$JUST_LOG")"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 2 ] || fail "not one retry: $(cat "$JUST_LOG")"
has "$XCRUN_LOG" 'simctl shutdown all'
has "$XCRUN_LOG" 'simctl erase 4AD2086B-7B8F-4608-A742-AA422A0B8D74'
[ "$(grep -c 'simctl erase' "$XCRUN_LOG")" = 1 ] || fail "erased another device: $(cat "$XCRUN_LOG")"
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
[ "$(sort -u "$JUST_LOG")" = "$first" ] || fail "the retry ran something else: $(cat "$JUST_LOG")"
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

# No such device, or its erase fails: no retry, the failed attempt's
# .xcresult left for the Failing tests step.
XCRUN_NO_DEVICE=1 run nodevice 'ui 1' "fail $runner_json" pass
[ "$rc" = 65 ] || fail "no device exited $rc: $(cat "$out")"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 1 ] || fail "no device retried: $(cat "$JUST_LOG")"
has "$out" "::warning::no iPhone 17 on iOS $IOS_SIM_BUILDS_RUNTIME to reset, so no retry"
[ -d "$dir/target/ios/DerivedData/Logs/Test/Test-mdoc-1.xcresult" ] || fail "no device moved the xcresult"
XCRUN_ERASE_FAIL=1 run noerase 'ui 1' "fail $runner_json" pass
[ "$rc" = 65 ] || fail "a failed erase exited $rc: $(cat "$out")"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 1 ] || fail "a failed erase retried: $(cat "$JUST_LOG")"
has "$out" '::warning::could not erase iPhone 17 (4AD2086B-7B8F-4608-A742-AA422A0B8D74), so no retry'
[ -d "$dir/target/ios/DerivedData/Logs/Test/Test-mdoc-1.xcresult" ] || fail "a failed erase moved the xcresult"
# The .xcresult cannot move aside: no retry, so the Failing tests step never
# reads the first attempt's error as the retry's.
moved="$tmp/nomove/target/ios/DerivedData/Logs/RunnerFailed"
mkdir -p "$moved" && chmod 555 "$moved"
run nomove 'ui 1' "fail $runner_json" pass
chmod 755 "$moved"
[ "$rc" = 65 ] || fail "a failed move exited $rc: $(cat "$out")"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 1 ] || fail "a failed move retried: $(cat "$JUST_LOG")"

# The unit part's runner fails to start: unit tests never retry.
run unit unit "fail $runner_json" pass
[ "$rc" = 65 ] || fail "the unit part exited $rc: $(cat "$out")"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 1 ] || fail "the unit part retried: $(cat "$JUST_LOG")"

# A build failure (no .xcresult): no retry.
run build 'ui 3' 'exit 65' pass
[ "$rc" = 65 ] || fail "a build failure exited $rc: $(cat "$out")"
[ "$(wc -l <"$JUST_LOG" | tr -d ' ')" = 1 ] || fail "a build failure retried: $(cat "$JUST_LOG")"

# The ios-test recipe's line, as just runs it (the device and tests in its
# place), from the repository root on the fake xcodebuild: `test`, or the
# action MDOC_IOS_TEST_ACTION names, on the same tests and device.
recipe=$(awk '/^ios-test device=/ { getline; sub(/^    /, ""); print; exit }' justfile |
  sed -e 's/{{device}}/iPhone 17/' -e 's#{{only}}#mdocUITests/AUITests mdocUITests/BUITests#')
case "$recipe" in *xcodebuild*) ;; *) fail "no ios-test recipe line: $recipe" ;; esac
export XCODEBUILD_LOG="$tmp/xcodebuild.log"
only='-only-testing:mdocUITests/AUITests -only-testing:mdocUITests/BUITests'
bash -c "$recipe" || fail "the recipe failed"
grep -q -- " -quiet test $only" "$XCODEBUILD_LOG" || fail "the recipe did not test: $(cat "$XCODEBUILD_LOG")"
grep -q -- "-destination platform=iOS Simulator,OS=$IOS_SIM_BUILDS_RUNTIME,name=iPhone 17 " "$XCODEBUILD_LOG" ||
  fail "the recipe tests another device: $(cat "$XCODEBUILD_LOG")"
: >"$XCODEBUILD_LOG"
MDOC_IOS_TEST_ACTION=test-without-building bash -c "$recipe" || fail "the recipe failed without building"
grep -q -- " -quiet test-without-building $only" "$XCODEBUILD_LOG" || fail "the recipe built: $(cat "$XCODEBUILD_LOG")"

# Usage.
rc=0
bash scripts/ios/ios-test-part.sh >"$tmp/usage" 2>&1 || rc=$?
[ "$rc" = 2 ] || fail "usage exited $rc"
grep -q '^usage: scripts/ios/ios-test-part.sh <part>' "$tmp/usage" || fail "no usage: $(cat "$tmp/usage")"
echo "test-ios-test-part: ok"
