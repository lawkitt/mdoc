#!/usr/bin/env bash
# Usage: scripts/test-ci-stats.sh
#
# Checks scripts/ci-stats.sh against fake GitHub answers: the summaries,
# each target's verdict, and the infrastructure wording. Bash 3.2
# compatible.
cd "$(dirname "$0")/.." || exit 1
CI_STATS_SOURCED=1 source scripts/ci-stats.sh

failures=0
fail() {
  echo "FAIL: $*"
  failures=$((failures + 1))
}
eq() { [ "$1" = "$2" ] || fail "expected '$2', got '$1'"; }

eq "$(printf '%s\n' 3 1 2 | summary)" "3 2.0 3.0"
eq "$(printf '%s\n' 10 20 30 40 | summary)" "4 25.0 40.0"
eq "$(summary </dev/null)" "0 - -"
echo "ok: medians and p90"

within 12.5 13 || fail "12.5 is within 13"
if within 13.5 13; then fail "13.5 is not within 13"; fi
within - 13 || fail "no samples hold"
echo "ok: a median within its limit"

eq "$(printf 'pending 1000 Waiting: macos\nsuccess 1660 Passed: macos (hosted)\n' | queue_minutes 1000)" "11.0"
eq "$(printf 'pending 1000 Waiting\nfailure 1300 Failed: ios (hosted)\n' | queue_minutes 1000)" ""
printf 'failure 1300 Failed: ios (no macOS runner, 3 attempts)\n' | infra_failure || fail "no macOS runner is infrastructure"
if printf 'failure 1300 Failed: ios (hosted)\n' | infra_failure; then fail "a test failure is not infrastructure"; fi
echo "ok: a queue commit's minutes and its infrastructure failure"

# A whole report on fakes: two queue commits (11 and 14 min, the second an
# infrastructure failure first), three PR heads (3, 4, 6 min).
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
out=$(report 7) && rc=0 || rc=$?
eq "$rc" 0
eq "$(sed -n 1p <<<"$out")" "queue  pass  queue commit to green gate: 2 commits, median 12.5 min, p90 14.0 min (target: median 13 or less)"
eq "$(sed -n 2p <<<"$out")" "fast   pass  PR head to fast tier: 3 runs, median 4.0 min, p90 6.0 min (target: median 4 or less)"
eq "$(sed -n 3p <<<"$out")" "infra  pass  infrastructure failures: 1 in 7 days, 1.0 a week (target: 1 or fewer)"
echo "ok: a report whose targets hold"

out=$(report 3) && rc=0 || rc=$?
eq "$rc" 1
eq "$(sed -n 3p <<<"$out")" "infra  FAIL  infrastructure failures: 1 in 3 days, 2.3 a week (target: 1 or fewer)"
echo "ok: a target that fails fails the report"

if [ "$failures" -gt 0 ]; then
  echo "error: $failures ci-stats check(s) failed" >&2
  exit 1
fi
echo "ci-stats: all checks passed"
