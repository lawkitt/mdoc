#!/usr/bin/env bash
# Usage: scripts/ios/ios-test-part.sh <part> [<test>...]
# ci-ios.yml's Tests step for one part (`unit`, `ui 1`, ...):
# `just --no-deps ios-app ios-test "iPhone 17" <test>...`, from the
# repository root on its Mac, exiting with its status, but for a UI part
# whose test runner failed to start (#1065): the preview xcode-27 image's
# runner sometimes crashes while preparing to run tests, or times out
# initializing for UI testing, before any test, and the per-test retry
# (MDOC_IOS_TEST_RETRY) never sees a test fail. When no test case ran and
# the runner's own error is the only failure (scripts/ios/ios-failed-tests.py
# runner), the part runs once more: the failed attempt's .xcresult moves to
# target/ios/DerivedData/Logs/RunnerFailed (the later steps read the
# retry's), the booted Simulator is shut down and erased, and
# `ios-test` runs the same tests with `test-without-building`
# (MDOC_IOS_TEST_ACTION), so the app is not rebuilt. A pass keeps the part
# green with a warning, `ci/ios (<part>): UI test runner failed to start;
# retried once`, an infrastructure failure that charges no test; a failure
# fails the part as before. A failure after a test started, and the unit
# part, never retry (docs/specs/ci.md: unit tests do not retry), so no test
# result is hidden. Bash 3.2 compatible.
set -uo pipefail
if [ "$#" -lt 1 ]; then
  echo "usage: scripts/ios/ios-test-part.sh <part> [<test>...]" >&2
  exit 2
fi
part=$1
shift
device="iPhone 17"
logs=target/ios/DerivedData/Logs

rc=0
just --no-deps ios-app ios-test "$device" "$@" || rc=$?
[ "$rc" -ne 0 ] || exit 0
case "$part" in
  "ui "*) ;;
  *) exit "$rc" ;;
esac
python3 "$(dirname "$0")/ios-failed-tests.py" runner "$logs/Test" || exit "$rc"

echo "The UI test runner failed to start, before any test: resetting the Simulator and testing once more without building (#1065)"
mkdir -p "$logs/RunnerFailed"
mv "$logs/Test/"*.xcresult "$logs/RunnerFailed/"
booted=$(xcrun simctl list devices booted | grep -oE '[0-9A-F]{8}(-[0-9A-F]{4}){3}-[0-9A-F]{12}')
xcrun simctl shutdown all || true
for udid in $booted; do
  xcrun simctl erase "$udid" || echo "::warning::could not erase the Simulator $udid; retrying on it as it is"
done

MDOC_IOS_TEST_ACTION=test-without-building just --no-deps ios-test "$device" "$@" || exit
note="ci/ios ($part): UI test runner failed to start; retried once"
echo "::warning::$note"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### $note"
    echo
    echo "No test had run when the runner failed, so the retry hides no test result: an infrastructure failure, charged to no test."
    echo
  } >>"$GITHUB_STEP_SUMMARY"
fi
