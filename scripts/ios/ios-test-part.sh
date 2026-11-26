#!/usr/bin/env bash
# Usage: scripts/ios/ios-test-part.sh <part> [<test>...]
# ci-ios.yml's Tests step for one part (`unit`, `ui 1`, ...):
# `just --no-deps ios-test "iPhone 17" <test>...` with
# `test-without-building` (MDOC_IOS_TEST_ACTION), on the app the core job
# built for testing (`just ios-app`, downloaded to target/ios/DerivedData,
# #1076), from the repository root on its Mac, exiting with its status.
#
# A UI part first warms the test device up (the iPhone 17 of the pinned
# runtime, scripts/tools.env): boots it and launches the app once, so the
# first test's launch is not a cold Simulator's, which took over a minute
# and failed the shard's first test on both its tries (#1226). The warm-up
# is best effort: when the device or the app is missing, or a step fails or
# runs past its limit, it says so and the tests run as before.
#
# A UI part also runs once more when its test runner never got to a test,
# as the per-test retry (MDOC_IOS_TEST_RETRY) never sees a test fail:
#   - it failed to start (#1065): the preview xcode-27 image's runner
#     sometimes crashes while preparing to run tests, or times out
#     initializing for UI testing; no test case ran and the runner's own
#     error is the only failure (scripts/ios/ios-failed-tests.py runner);
#   - it hung (#1215, #1226): no test case started within
#     MDOC_IOS_FIRST_TEST_LIMIT seconds (5 minutes; after the warm-up the
#     first test started 1 to 2.2 minutes in on hosted runners). The job
#     then sat silent until its 45-minute limit. A test case has started
#     once the runner's output, staged in the result bundle while it runs,
#     says so; past the limit xcodebuild is stopped and the end of its
#     session log printed. A run counts as hung only when, stopped, it
#     still shows no test started, so a test that started at the limit
#     keeps its result.
# The device is then shut down, erased and warmed up again, the failed
# attempt's .xcresult moves to target/ios/DerivedData/Logs/RunnerFailed (the
# later steps read the retry's), and `ios-test` runs the same tests again.
# When the device cannot be found or erased, the .xcresult moved, or more
# than MDOC_IOS_RETRY_BUDGET seconds (15 minutes) have gone, so that a
# shard's tests (20 to 30 minutes) would not fit in the job's 45 before the
# retry starts, it does not retry and fails as before. A pass keeps the
# part green with a warning, `ci/ios (<part>): UI test runner failed to
# start; retried once` (or `hung before the first test`), an
# infrastructure failure that charges no test; a failure fails the part as
# before, and a second hang fails it naming the hang. A failure or hang
# after a test started, and the unit part, never retry (docs/specs/ci.md:
# unit tests do not retry), so no test result is hidden. Bash 3.2
# compatible.
set -uo pipefail
if [ "$#" -lt 1 ]; then
  echo "usage: scripts/ios/ios-test-part.sh <part> [<test>...]" >&2
  exit 2
fi
part=$1
shift
device="iPhone 17"
logs=target/ios/DerivedData/Logs
app=target/ios/DerivedData/Build/Products/Debug-iphonesimulator/mdoc.app
bundle=io.github.lawkitt.mdoc
first_test_limit=${MDOC_IOS_FIRST_TEST_LIMIT:-300}
retry_budget=${MDOC_IOS_RETRY_BUDGET:-900}
ui=
case "$part" in "ui "*) ui=1 ;; esac
# The watch's mark that it stopped a hung run.
hung=$(mktemp "${TMPDIR:-/tmp}/ios-test-part-hung.XXXXXX") || exit 1
rm -f "$hung"

# ci-ios.yml looks for this line: a commit without it builds the app in
# each part, which needs the core there.
export MDOC_IOS_TEST_ACTION=test-without-building

# limited <seconds> <command>...: the command's status, or 124 when it ran
# past <seconds> and its alarm stopped it (macOS has no `timeout`).
limited() {
  local rc=0
  perl -e 'alarm shift; exec @ARGV or exit 127' "$@" || rc=$?
  [ "$rc" != 142 ] || return 124
  return "$rc"
}

# warm_up <udid>: boots the device and launches the app once, with the UI
# tests' flag, which gives it a library of its own (apps/ios/ui-tests.md).
warm_up() {
  local udid=$1 start=$SECONDS booted
  if ! limited 300 xcrun simctl bootstatus "$udid" -b >/dev/null; then
    echo "Simulator warm-up: $device ($udid) did not boot within 5 minutes; testing without the warm-up"
    return 0
  fi
  booted=$((SECONDS - start))
  if ! [ -d "$app" ]; then
    echo "Simulator warm-up: $device booted in ${booted}s; no app at $app to launch"
    return 0
  fi
  if limited 120 xcrun simctl install "$udid" "$app" &&
    limited 120 xcrun simctl launch "$udid" "$bundle" -mdoc-ui-testing >/dev/null; then
    echo "Simulator warm-up: $device booted in ${booted}s; the app installed and launched in $((SECONDS - start - booted))s"
  else
    echo "Simulator warm-up: $device booted in ${booted}s; the app did not install and launch within 2 minutes each"
  fi
}

# Whether a test case has started: the runner's output, which the result
# bundle stages while the tests run.
started() {
  grep -qs "^Test Case '.*' started" \
    "$logs"/Test/*.xcresult/Staging/*/Diagnostics/*/*/StandardOutputAndStandardError*.txt
}

# watch <pid>: until a test case starts, looks five times a second; past
# the limit, marks the run hung and stops the process group <pid> leads.
watch() {
  local start=$SECONDS
  while ! started; do
    if [ $((SECONDS - start)) -ge "$first_test_limit" ]; then
      echo "No UI test started within ${first_test_limit}s: stopping xcodebuild. The end of its session log:"
      tail -n 15 "$logs"/Test/*.xcresult/Staging/*/Diagnostics/*/Session-*.log 2>/dev/null ||
        echo "(none yet: xcodebuild had made no result bundle)"
      : >"$hung"
      kill -TERM -- "-$1" 2>/dev/null || kill -TERM "$1" 2>/dev/null
      return
    fi
    sleep 0.2
  done
  echo "ci/ios ($part): the first UI test started $((SECONDS - start))s in"
}

# tests: `ios-test` on the part's tests, its status; for a UI part, 124
# when no test case started within the limit and xcodebuild was stopped.
# xcodebuild runs in a process group of its own (just runs it in a shell),
# so all of it stops.
tests() {
  local pid watcher='' rc=0
  rm -f "$hung"
  set -m
  just --no-deps ios-test "$device" "$@" &
  pid=$!
  set +m
  if [ -n "$ui" ]; then
    watch "$pid" &
    watcher=$!
  fi
  wait "$pid" || rc=$?
  if [ -n "$watcher" ]; then
    kill "$watcher" 2>/dev/null
    wait "$watcher" 2>/dev/null
  fi
  # A run that passed as the limit came, or whose first test started
  # then, was no hang: its result stands.
  if [ -f "$hung" ] && [ "$rc" -ne 0 ] && ! started; then
    rc=124
  fi
  rm -f "$hung"
  return "$rc"
}

# The destination `ios-test` names: the device in its runtime's section
# (`-- iOS 27.2 --`), as `    iPhone 17 (<udid>) (Booted)`, booted or not.
udid=
if [ -n "$ui" ]; then
  os=$(source "$(dirname "$0")/../lib.sh" && load_tools && echo "$IOS_SIM_RUNTIME") || os=
  if [ -n "$os" ]; then
    udid=$(xcrun simctl list devices available |
      awk -v section="-- iOS $os --" -v name="    $device (" '/^-- / { here = ($0 == section) } here && index($0, name) == 1' |
      grep -oE '[0-9A-F]{8}(-[0-9A-F]{4}){3}-[0-9A-F]{12}' | head -n 1)
  fi
  if [ -n "$udid" ]; then
    warm_up "$udid"
  else
    echo "Simulator warm-up: no $device on iOS ${os:-?} to warm up"
  fi
fi

rc=0
tests "$@" || rc=$?
[ "$rc" -ne 0 ] || exit 0
[ -n "$ui" ] || exit "$rc"
if [ "$rc" = 124 ]; then
  what="hung before the first test"
else
  python3 "$(dirname "$0")/ios-failed-tests.py" runner "$logs/Test" || exit "$rc"
  what="failed to start"
fi

if [ "$SECONDS" -gt "$retry_budget" ]; then
  echo "::error::ci/ios ($part): UI test runner $what ${SECONDS}s in, too late for a retry to finish within the job's limit"
  [ "$rc" != 124 ] || exit 1
  exit "$rc"
fi
echo "The UI test runner $what: resetting the Simulator and testing once more without building (#1065, #1226)"
if [ -z "$udid" ]; then
  echo "::warning::no $device on iOS ${os:-?} to reset, so no retry"
  exit "$rc"
fi
limited 120 xcrun simctl shutdown all || true
if ! limited 120 xcrun simctl erase "$udid"; then
  echo "::warning::could not erase $device ($udid), so no retry"
  exit "$rc"
fi
mkdir -p "$logs/RunnerFailed" || exit "$rc"
# A hang can come before xcodebuild made its result bundle.
if [ "$rc" != 124 ] || ls -d "$logs/Test/"*.xcresult >/dev/null 2>&1; then
  mv "$logs/Test/"*.xcresult "$logs/RunnerFailed/" || exit "$rc"
fi
warm_up "$udid"

rc=0
tests "$@" || rc=$?
if [ "$rc" = 124 ]; then
  echo "::error::ci/ios ($part): UI test runner hung twice before the first test"
  exit 1
fi
[ "$rc" = 0 ] || exit "$rc"
note="ci/ios ($part): UI test runner $what; retried once"
echo "::warning::$note"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### $note"
    echo
    echo "No test had run when the runner failed or hung, so the retry hides no test result: an infrastructure failure, charged to no test."
    echo
  } >>"$GITHUB_STEP_SUMMARY"
fi
