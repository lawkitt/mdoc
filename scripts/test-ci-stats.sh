#!/usr/bin/env bash
# Usage: scripts/test-ci-stats.sh
#
# Checks scripts/ci-stats.sh against fake GitHub answers: the summaries,
# each target's verdict at its boundary, the infrastructure wording and
# rate, and the day-by-day run listing. Bash 3.2 compatible.
cd "$(dirname "$0")/.." || exit 1
CI_STATS_SOURCED=1 source scripts/ci-stats.sh

failures=0
fail() {
  echo "FAIL: $*"
  failures=$((failures + 1))
}
eq() { [ "$1" = "$2" ] || fail "expected '$2', got '$1'"; }

eq "$(printf '%s\n' 3 1 2 | summary)" "3 2 3"
eq "$(printf '%s\n' 10 20 30 40 | summary)" "4 25 40"
eq "$(summary </dev/null)" "0 - -"
eq "$(minutes 750)" "12.5"
eq "$(minutes -)" "-"
echo "ok: medians, p90 and minutes"

# Unrounded: 13m01s fails a 13-minute target, 13m00s holds.
within 780 13 || fail "13m00s is within 13"
if within 781 13; then fail "13m01s is not within 13"; fi
if within 241 4; then fail "4m01s is not within 4"; fi
within - 13 || fail "no samples hold"
echo "ok: a median within its limit, unrounded"

eq "$(printf 'pending 1000 Waiting: macos\nsuccess 1660 Passed: macos (hosted)\n' | queue_seconds 1000)" "660"
eq "$(printf 'pending 1000 Waiting\nfailure 1300 Failed: ios (hosted)\n' | queue_seconds 1000)" ""
printf 'failure 1300 Failed: ios (no macOS runner, 3 attempts)\n' | infra_failure || fail "no macOS runner is infrastructure"
if printf 'failure 1300 Failed: ios (hosted)\n' | infra_failure; then fail "a test failure is not infrastructure"; fi
echo "ok: a queue commit's time and its infrastructure failure"

# Runs listed a day at a time (GitHub caps a filtered list at 1,000), each
# run once though two days' answers overlap at their shared second.
slices=$(mktemp)
fetch_runs() {
  echo "$1 $2 $3 $4" >>"$slices"
  printf '7 sha7 %s\n' 100
}
eq "$(runs_between ci.yml 0 $((2 * 86400 + 60)) pull_request)" "7 sha7 100"
eq "$(wc -l <"$slices" | tr -d ' ')" 3
eq "$(sed -n 3p "$slices")" "ci.yml 1970-01-03T00:00:00Z 1970-01-03T00:01:00Z pull_request"
rm -f "$slices"
echo "ok: a long window is listed a day at a time"

# A whole report on fakes over 7 days: two queue commits (11 and 14 min, the
# second an infrastructure failure first), three PR heads (3, 4, 6 min).
fetch_runs() {
  case "$1" in
    queue.yml) printf '1 aaa 1000\n2 bbb 2000\n' ;;
    ci.yml) printf '11 h1 100\n12 h2 200\n13 h3 300\n' ;;
  esac
}
fetch_gate() {
  case "$1" in
    aaa) printf 'pending 1000 Waiting\nsuccess 1660 Passed\n' ;;
    bbb) printf 'failure 2300 Failed: ios (no macOS runner, 3 attempts)\nsuccess 2840 Passed\n' ;;
  esac
}
fetch_fast_done() {
  case "$1" in
    11) echo 280 ;;
    12) echo 440 ;;
    13) echo 660 ;;
  esac
}
week=$((7 * 86400))
out=$(report 0 "$week") && rc=0 || rc=$?
eq "$rc" 0
eq "$(sed -n 1p <<<"$out")" "queue  pass  queue commit to green gate: 2 commits, median 12.5 min, p90 14.0 min (target: median 13 or less)"
eq "$(sed -n 2p <<<"$out")" "fast   pass  PR head to fast tier: 3 runs, median 4.0 min, p90 6.0 min (target: median 4 or less)"
eq "$(sed -n 3p <<<"$out")" "infra  pass  infrastructure failures: 1 in 7.0 days, 1.00 a week (target: 1 or fewer)"
echo "ok: a report whose targets hold"

# One failure in 6 days and an hour is 1.16 a week: it fails, unrounded.
out=$(report 0 $((6 * 86400 + 3600))) && rc=0 || rc=$?
eq "$rc" 1
eq "$(sed -n 3p <<<"$out")" "infra  FAIL  infrastructure failures: 1 in 6.0 days, 1.16 a week (target: 1 or fewer)"
echo "ok: a target that fails fails the report"

if [ "$failures" -gt 0 ]; then
  echo "error: $failures ci-stats check(s) failed" >&2
  exit 1
fi
echo "ci-stats: all checks passed"
