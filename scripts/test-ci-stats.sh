#!/usr/bin/env bash
# Usage: scripts/test-ci-stats.sh
#
# Checks scripts/ci-stats.sh against fake GitHub answers: the summaries, each
# target's verdict at its boundary, what merge-gate and the PC's statuses say,
# each queue commit's class, the iOS UI shards, each target's start,
# which drops count as not caused by the PR, the day-by-day run listing, whole
# reports over one window and several nights, and scripts/ci-targets-issue.sh
# opening, commenting on and closing its issue. Bash 3.2 compatible.
cd "$(dirname "$0")/.." || exit 1
root=$(pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
printf '%s\n' \
  '# a comment naming tests::not_this one' \
  'android:io.github.x.ATest#flaky  #1  2099-01-01' \
  'package(mdoc-markdown) & test(=from_html::tests::flaky)  #2  2099-01-01' \
  'test(=deep::slow)  #3  2099-01-01' >"$tmp/quarantine"
export CI_STATS_QUARANTINE=$tmp/quarantine
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

# Unrounded: 25m01s fails a 25-minute target, 25m00s holds.
within 1500 25 || fail "25m00s is within 25"
if within 1501 25; then fail "25m01s is not within 25"; fi
if within 901 15; then fail "15m01s is not within 15"; fi
within - 20 || fail "no samples hold"
echo "ok: a median within its limit, unrounded"

eq "$(printf 'pending 1000 Waiting: macos\nsuccess 1660 Passed: macos (hosted)\n' | queue_seconds 1000)" "660 1660"
eq "$(printf 'pending 1000 Waiting\nfailure 1300 Failed: ios (hosted)\n' | queue_seconds 1000)" ""
eq "$(printf 'pending 1000 Waiting\nfailure 1300 Failed: ios (hosted); passed: macos (hosted)\nfailure 1400 Failed: x (hosted)\n' | gate_failure)" \
  "1300 Failed: ios (hosted); passed: macos (hosted)"
eq "$(printf 'success 1300 Passed: ios (hosted)\n' | gate_failure)" ""
echo "ok: a queue commit's time to green and its first failure"

eq "$(failed_jobs 'Failed: windows (PC), ios (no macOS runner, 3 attempts); passed: macos (hosted)')" \
  "windows PC
ios no macOS runner, 3 attempts"
eq "$(failed_jobs 'Failed: scripts (did not run)')" "scripts did not run"
eq "$(failed_jobs 'Waiting: macos')" ""
infra_where "no macOS runner, 3 attempts" || fail "no macOS runner is infrastructure"
infra_where "did not run, 3 attempts" || fail "did not run is infrastructure"
infra_where "did not run" || fail "did not run alone is infrastructure"
infra_where "cancelled on macOS, 3 attempts" || fail "cancelled after it started is infrastructure"
eq "$(failed_jobs 'Failed: ios (cancelled on macOS, 3 attempts)')" "ios cancelled on macOS, 3 attempts"
if infra_where hosted; then fail "a hosted failure is not infrastructure"; fi
if infra_where PC; then fail "a PC failure is not infrastructure"; fi
echo "ok: the jobs a failing merge-gate names, and its infrastructure wording"

eq "$(pc_test 'Failed in just test: ( 714/1634) mdoc-markdown from_html::tests::x and 2 more')" "1 mdoc-markdown from_html::tests::x"
eq "$(pc_test 'Failed in just android: android:a.B#c')" "0 android:a.B#c"
eq "$(pc_test 'Failed in just test')" ""
eq "$(pc_test 'Timed out in just test after 30m00s')" ""
echo "ok: the PC's failing test"

# The slower UI shard of a green pair, or the old single job.
jobs='success 1000 2000 ios / ci/ios (unit)
success 1000 1840 ios / ci/ios (ui 1)
success 1100 1500 ios / ci/ios (ui 2)
success 2141 2145 ios / ci/ios'
eq "$(ios_ui_seconds <<<"$jobs")" "840 1840"
eq "$(sed 3s/1100/1700/ <<<"$jobs" | ios_ui_seconds)" "840 1840"
eq "$(sed 3s/1500/2200/ <<<"$jobs" | ios_ui_seconds)" "1100 2200"
eq "$(sed 3s/success/failure/ <<<"$jobs" | ios_ui_seconds)" ""
eq "$(sed 2s/success/cancelled/ <<<"$jobs" | ios_ui_seconds)" ""
eq "$(sed 3s/1500/0/ <<<"$jobs" | ios_ui_seconds)" ""
eq "$(sed 3d <<<"$jobs" | ios_ui_seconds)" ""
eq "$(printf '%s\n' "$jobs" 'success 1000 1200 ios / ci/ios (ui)' | ios_ui_seconds)" ""
jobs='success 1000 2000 ios / ci/ios (unit)
success 1000 2140 ios / ci/ios (ui)'
eq "$(ios_ui_seconds <<<"$jobs")" "1140 2140"
eq "$(sed 2s/success/failure/ <<<"$jobs" | ios_ui_seconds)" ""
eq "$(echo 'success 1 2 macos / ci/macos (tests)' | ios_ui_seconds)" ""
echo "ok: the slower iOS UI shard, and the single job before it"

# A queue commit's class from the jobs it started.
eq "$(printf '%s\n' 'success 10 20 ios / ci/ios (unit)' 'failure 11 30 macos / ci/macos (tests)' \
  'success 12 13 detection / ci/detection (3)' 'success 14 15 detection / detection shards' \
  'success 16 17 areas' 'skipped 18 18 scripts' | class_jobs)" "10 ios
11 desktop
12 desktop"
eq "$(printf '%s\n' 'skipped 5 5 ios / ci/ios (ui 1)' 'none 0 0 macos / ci/macos (tests)' | class_jobs)" ""
printf '%s\n' \
  "100 core 100 other" "100 core 110 ios" "100 core 120 desktop" \
  "100 desk 100 other" "100 desk 130 desktop" \
  "200 det 200 other" "200 det 210 desktop" \
  "200 ios 200 other" "200 ios 210 ios" \
  "300 docs 300 other" \
  "300 late 300 other" "300 late 310 ios" "300 late 900 desktop" \
  "1000 after 1000 other" >"$tmp/classes"
eq "$(commit_classes "$tmp/classes" 0 899 | tr '\n' ' ')" \
  "core core desk desktop-only det desktop-only docs other ios iOS-only late iOS-only "
eq "$(commit_classes "$tmp/classes" 0 2000 | grep late)" "late core"
eq "$(commit_classes "$tmp/classes" 150 250 | tr '\n' ' ')" "det desktop-only ios iOS-only "
echo "ok: a queue commit's class (core, desktop-only, iOS-only, other) by the jobs it ran"

# Each target's start: pending reads the whole window and is not judged; a
# date reads from itself or from the window's start, whichever is later.
target_start_real=$(declare -f target_start)
target_start() { echo "$t_start"; }
t_start=pending
eq "$(target_from core 500)" "500 not judged: start date pending"
t_start=1970-01-01T00:10:00Z
eq "$(target_from core 500)" "600 since 1970-01-01T00:10:00Z"
eq "$(target_from core 700)" "700 since 1970-01-01T00:10:00Z"
t_start=1970-13-01
if (target_from core 0) >/dev/null 2>&1; then fail "a bad start date is an error"; fi
eval "$target_start_real"
for t in core desktop queue ios-ui drops; do
  d=$(target_start "$t")
  [ "$d" = pending ] || [[ $d =~ ^20[0-9]{2}-[0-9]{2}-[0-9]{2}T[0-9:]{8}Z$ ]] || fail "$t's start is pending or a UTC time: $d"
done
echo "ok: each target's start"

quarantined "android:io.github.x.ATest#flaky" || fail "an app UI line by name"
if quarantined "android:io.github.x.ATest#flak"; then fail "an app UI line by its whole name only"; fi
quarantined "mdoc-markdown from_html::tests::flaky" || fail "a Rust test its filter names"
if quarantined "mdoc-markdown from_html::tests::flaky_too"; then fail "a Rust test by its whole path only"; fi
if quarantined "mdoc-core tests::not_this"; then fail "a comment is no line"; fi
if quarantined "mdoc-core from_html::tests::flaky"; then fail "a Rust line naming another package"; fi
quarantined "mdoc-core deep::slow" || fail "a Rust line naming no package"
if quarantined "mdoc-core a::deep::slow"; then fail "a Rust path by its whole name, :: included"; fi
if CI_STATS_QUARANTINE=$tmp/none quarantine=$tmp/none quarantined "android:io.github.x.ATest#flaky"; then
  fail "no list quarantines nothing"
fi
echo "ok: the quarantine list"

eq "$(branch_pr gh-readonly-queue/main/pr-823-b7560a90303df74d258bd2373d213a0b97cc5744)" 823
eq "$(branch_pr ci-shadow/main/trial)" -
day=86400
printf '%s\n' \
  "1000 aaa 10 windows 0 mdoc-core tests::flake" \
  "$((1000 + 10 * day)) bbb 11 windows 0 mdoc-core tests::flake" \
  "$((1000 + 20 * day)) ccc 12 windows 0 mdoc-core tests::late" \
  "1000 ddd 13 android 0 android:io.github.x.ATest#flaky" \
  "1000 eee 14 windows 0 mdoc-core tests::own" \
  "2000 fff 14 windows 0 mdoc-core tests::own" \
  "1000 ggg 16 android 1 android:io.github.x.ATest#flaky" >"$tmp/tests"
end=$((30 * day))
eq "$(drop_reason aaa 10 1000 'Failed: windows (PC)' "$tmp/tests" "$end")" \
  "repeated (mdoc-core tests::flake, on another PR's queue commit)"
# Evidence posted after the cutoff does not count, nor the drop's own.
eq "$(drop_reason aaa 10 1000 'Failed: windows (PC)' "$tmp/tests" $((1000 + 10 * day - 1)))" related
eq "$(drop_reason aaa 10 1000 'Failed: windows (PC)' "$tmp/tests" 999)" related
eq "$(drop_reason ccc 12 $((1000 + 20 * day)) 'Failed: windows (PC)' "$tmp/tests" "$end")" related
eq "$(drop_reason eee 14 1000 'Failed: windows (PC)' "$tmp/tests" "$end")" related
eq "$(drop_reason ddd 13 1000 'Failed: android (PC); passed: macos (hosted)' "$tmp/tests" "$end")" \
  "quarantined (android:io.github.x.ATest#flaky)"
# A quarantined test and others unknown: the others may be the PR's.
eq "$(drop_reason ggg 16 1000 'Failed: android (PC)' "$tmp/tests" "$end")" related
eq "$(drop_reason zzz 15 1000 'Failed: ios (did not run, 3 attempts)' "$tmp/tests" "$end")" \
  "infrastructure (ios did not run, 3 attempts)"
eq "$(drop_reason zzz 15 1000 'Failed: ios (cancelled on macOS, 3 attempts)' "$tmp/tests" "$end")" \
  "infrastructure (ios cancelled on macOS, 3 attempts)"
eq "$(drop_reason zzz 15 1000 'Failed: ios (no macOS runner, 3 attempts), windows (hosted)' "$tmp/tests" "$end")" related
eq "$(drop_reason aaa 10 1000 'Failed: windows (PC), android (PC)' "$tmp/tests" "$end")" related
eq "$(drop_reason aaa 10 1000 'Failed: ios (hosted)' "$tmp/tests" "$end")" related
eq "$(drop_reason aaa 10 1000 'Stopped' "$tmp/tests" "$end")" related
echo "ok: which drops the PR did not cause, from what was known by a cutoff"

printf '%s\n' "100 60 160" "200 70 400" "300 80 350" >"$tmp/samples"
eq "$(samples "$tmp/samples" 100 350 | tr '\n' ' ')" "60 80 "
eq "$(samples "$tmp/samples" 150 400 | tr '\n' ' ')" "70 80 "
echo "ok: a window's samples, known by its end"

# Runs listed a day at a time (GitHub caps a filtered list at 1,000), each
# run once though two days' answers overlap at their shared second.
slices=$tmp/slices
fetch_runs() {
  echo "$1 $2 $3 $4" >>"$slices"
  printf '7 sha7 %s b\n' 100
}
eq "$(runs_between ci.yml 0 $((2 * day + 60)) pull_request)" "7 sha7 100 b"
eq "$(wc -l <"$slices" | tr -d ' ')" 3
eq "$(sed -n 3p "$slices")" "ci.yml 1970-01-03T00:00:00Z 1970-01-03T00:01:00Z pull_request"
echo "ok: a long window is listed a day at a time"

# A week of fakes, every queue commit an hour into a day:
#   q1 (PR 1, day 1, core): green in 20 min; UI shards of 14 and 8 min.
#   q2 (PR 2, day 2, desktop-only: macOS and detection): green in 30 min
#     after a drop the PR did not cause (an iOS job that never ran).
#   q3 (PR 3, day 3, iOS-only): dropped on a Windows test the PC also failed
#     on q4; its single `ci/ios (ui)` job (before the shards) took 20 min.
#   q4 (PR 4, day 4, other: its jobs skipped): dropped on the same test,
#     and Android (no test).
#   q5 (PR 3, day 6, other): dropped on a quarantined Android test; its
#     re-run (same commit, later) is read once.
q() { echo $((($1 - 1) * day + 3600)); }
fetch_runs() {
  case "$1 $4" in
    "queue.yml merge_group")
      printf '%s\n' \
        "1 q1 $(q 1) gh-readonly-queue/main/pr-1-x" \
        "2 q2 $(q 2) gh-readonly-queue/main/pr-2-x" \
        "3 q3 $(q 3) gh-readonly-queue/main/pr-3-x" \
        "4 q4 $(q 4) gh-readonly-queue/main/pr-4-x" \
        "5 q5 $(q 6) gh-readonly-queue/main/pr-3-x" \
        "6 q5 $(($(q 6) + 600)) gh-readonly-queue/main/pr-3-x"
      ;;
  esac |
    awk -v f="$(jq -rn --arg s "$2" '$s | fromdateiso8601')" -v t="$(jq -rn --arg s "$3" '$s | fromdateiso8601')" '$3 >= f && $3 <= t'
}
fetch_statuses() {
  local s
  case "$1" in
    q1)
      s=$(q 1)
      printf '%s\n' "merge-gate success $((s + 1200)) Passed: macos (hosted), windows (PC), ios (hosted)" \
        "ci/windows success $((s + 900)) Passed in 14m00s" "merge-gate pending $((s + 5)) Waiting: macos, windows, ios"
      ;;
    q2)
      s=$(q 2)
      printf '%s\n' "merge-gate failure $((s + 600)) Failed: ios (did not run, 3 attempts); passed: windows (hosted)" \
        "merge-gate success $((s + 1800)) Passed: macos (hosted), detection (hosted)"
      ;;
    q3)
      s=$(q 3)
      printf '%s\n' "merge-gate failure $((s + 900)) Failed: windows (PC)" \
        "ci/windows failure $((s + 800)) Failed in just test-tier rust: ( 1/9) mdoc-core tests::flake"
      ;;
    q4)
      s=$(q 4)
      printf '%s\n' "merge-gate failure $((s + 900)) Failed: windows (PC), android (PC)" \
        "ci/windows failure $((s + 800)) Failed in just test-tier rust: ( 1/9) mdoc-core tests::flake and 1 more" \
        "ci/android failure $((s + 850)) Failed in just android"
      ;;
    q5)
      s=$(q 6)
      printf '%s\n' "merge-gate failure $((s + 900)) Failed: android (PC)" \
        "ci/android failure $((s + 850)) Failed in just android: android:io.github.x.ATest#flaky"
      ;;
  esac
}
fetch_jobs() {
  local s
  case "$1" in
    1)
      s=$(q 1)
      printf '%s\n' "success $((s + 10)) $((s + 850)) ios / ci/ios (ui 1)" "success $((s + 10)) $((s + 490)) ios / ci/ios (ui 2)" \
        "success $((s + 10)) $((s + 400)) ios / ci/ios (unit)" "success $((s + 10)) $((s + 700)) macos / ci/macos (tests)"
      ;;
    2)
      s=$(q 2)
      printf '%s\n' "success $((s + 10)) $((s + 900)) macos / ci/macos (tests)" "success $((s + 20)) $((s + 300)) detection / ci/detection (1)" \
        "skipped $((s + 5)) $((s + 5)) ios"
      ;;
    3)
      s=$(q 3)
      printf '%s\n' "success $((s + 10)) $((s + 1210)) ios / ci/ios (ui)" "success $((s + 10)) $((s + 400)) ios / ci/ios (unit)"
      ;;
    4)
      s=$(q 4)
      printf '%s\n' "success $((s + 1)) $((s + 9)) areas" "skipped $((s + 9)) $((s + 9)) ios / ci/ios (unit)" "skipped $((s + 9)) $((s + 9)) macos"
      ;;
    *) : ;;
  esac
}
# Every target reads from the epoch unless a case sets its start.
t_core=1970-01-01T00:00:00Z t_desktop=1970-01-01T00:00:00Z t_queue=1970-01-01T00:00:00Z
t_ios=1970-01-01T00:00:00Z t_drops=1970-01-01T00:00:00Z
target_start() {
  case "$1" in
    core) echo "$t_core" ;;
    desktop) echo "$t_desktop" ;;
    queue) echo "$t_queue" ;;
    ios-ui) echo "$t_ios" ;;
    drops) echo "$t_drops" ;;
  esac
}
dir=$tmp/week
mkdir "$dir"
week=$((7 * day))
gather 0 "$week" "$dir"
eq "$(wc -l <"$dir/queue" | tr -d ' ')" 2
eq "$(wc -l <"$dir/drops" | tr -d ' ')" 4
eq "$(cat "$dir/ios-ui")" "$(q 1) 840 $(($(q 1) + 850))
$(q 3) 1200 $(($(q 3) + 1210))"
eq "$(commit_classes "$dir/classes" 0 "$week" | tr '\n' ' ')" "q1 core q2 desktop-only q3 iOS-only q4 other q5 other "
since='since 1970-01-01T00:00:00Z'
out=$(report 0 "$week" "$dir") && rc=0 || rc=$?
eq "$rc" 1
eq "$(sed -n 1p <<<"$out")" "classes  queue commits: 1 core, 1 desktop-only, 1 iOS-only, 2 other"
eq "$(sed -n 2p <<<"$out")" "core     pass  core queue commits to green gate: 1, median 20.0 min, p90 20.0 min (target: median 20 or less; $since)"
eq "$(sed -n 3p <<<"$out")" "desktop  FAIL  desktop-only queue commits to green gate: 1, median 30.0 min, p90 30.0 min (target: median 15 or less; $since)"
eq "$(sed -n 4p <<<"$out")" "queue    pass  every queue commit to green gate: 2, median 25.0 min, p90 30.0 min (target: median 25 or less; $since)"
eq "$(sed -n 5p <<<"$out")" "ios-ui   FAIL  green iOS UI runs, the slower shard: 2, median 17.0 min, p90 20.0 min (target: median 15 or less; $since)"
eq "$(sed -n 6p <<<"$out")" "drops    FAIL  queue drops not caused by the PR: 3 of 4 drops in 7.0 days (1 infrastructure, 1 quarantined test, 1 repeated test), 3.00 a week (target: 1 or fewer; $since)"
eq "$(sed -n 7p <<<"$out")" "         1970-01-02T01:00:00Z q2 PR #2: infrastructure (ios did not run, 3 attempts)"
eq "$(sed -n 8p <<<"$out")" "         1970-01-03T01:00:00Z q3 PR #3: repeated (mdoc-core tests::flake, on another PR's queue commit)"
eq "$(sed -n 9p <<<"$out")" "         1970-01-06T01:00:00Z q5 PR #3: quarantined (android:io.github.x.ATest#flaky)"
eq "$(wc -l <<<"$out" | tr -d ' ')" 9
echo "ok: a week's report, each class, each target and the drops the PRs did not cause"

# A pending start reports the target but never fails the window.
out=$(t_desktop=pending t_ios=pending t_drops=pending report 0 "$week" "$dir") && rc=0 || rc=$?
eq "$rc" 0
eq "$(sed -n 3p <<<"$out")" "desktop  FAIL  desktop-only queue commits to green gate: 1, median 30.0 min, p90 30.0 min (target: median 15 or less; not judged: start date pending)"
eq "$(sed -n 5p <<<"$out" | cut -c1-14)" "ios-ui   FAIL "
grep -q '^drops    FAIL .*3.00 a week (target: 1 or fewer; not judged: start date pending)$' <<<"$out" || fail "a pending drops line"
# A start inside the window reads from it: the single job's 20-minute q3
# alone; the drops from day 4, over 4 days.
out=$(t_ios=1970-01-03T00:00:00Z t_drops=1970-01-04T00:00:00Z report 0 "$week" "$dir") && rc=0 || rc=$?
eq "$rc" 1
eq "$(sed -n 5p <<<"$out")" "ios-ui   FAIL  green iOS UI runs, the slower shard: 1, median 20.0 min, p90 20.0 min (target: median 15 or less; since 1970-01-03T00:00:00Z)"
eq "$(sed -n 6p <<<"$out")" "drops    FAIL  queue drops not caused by the PR: 1 of 2 drops in 4.0 days (0 infrastructure, 1 quarantined test, 0 repeated test), 1.75 a week (target: 1 or fewer; since 1970-01-04T00:00:00Z)"
# A window ending before a start holds with no samples.
out=$(t_core=1970-01-05T00:00:00Z t_drops=1970-01-05T00:00:00Z report 0 "$day" "$dir") && rc=0 || rc=$?
eq "$rc" 0
eq "$(sed -n 2p <<<"$out")" "core     pass  core queue commits to green gate: 0, median - min, p90 - min (target: median 20 or less; since 1970-01-05T00:00:00Z)"
grep -q '^drops    pass  queue drops not caused by the PR: 0 of 0 drops in 0.0 days' <<<"$out" || fail "drops before their start"
echo "ok: each target from its start, and a pending one not judged"

# Only q1: everything holds. The limits can be forced for a drill.
out=$(report 0 "$day" "$dir") && rc=0 || rc=$?
eq "$rc" 0
eq "$(sed -n 1p <<<"$out")" "classes  queue commits: 1 core, 0 desktop-only, 0 iOS-only, 0 other"
eq "$(sed -n 5p <<<"$out")" "ios-ui   pass  green iOS UI runs, the slower shard: 1, median 14.0 min, p90 14.0 min (target: median 15 or less; $since)"
eq "$(sed -n 6p <<<"$out")" "drops    pass  queue drops not caused by the PR: 0 of 0 drops in 1.0 days (0 infrastructure, 0 quarantined test, 0 repeated test), 0.00 a week (target: 1 or fewer; $since)"
out=$(core_min=19 report 0 "$day" "$dir") && rc=0 || rc=$?
eq "$rc" 1
eq "$(sed -n 2p <<<"$out")" "core     FAIL  core queue commits to green gate: 1, median 20.0 min, p90 20.0 min (target: median 19 or less; $since)"
out=$(queue_min=0 report 0 "$day" "$dir") && rc=0 || rc=$?
eq "$rc" 1
eq "$(sed -n 4p <<<"$out")" "queue    FAIL  every queue commit to green gate: 1, median 20.0 min, p90 20.0 min (target: median 0 or less; $since)"
out=$(drill=1 report "$day" $((day + 60)) "$dir") && rc=0 || rc=$?
eq "$rc" 1
eq "$(sed -n 4p <<<"$out")" "queue    pass  every queue commit to green gate: 0, median - min, p90 - min (target: median 25 or less; $since)"
eq "$(tail -n 1 <<<"$out")" "drill    FAIL  forced red by CI_STATS_DRILL, a drill of the nightly's issue"
echo "ok: a window whose targets hold, a target forced red, and a drill with no samples"

# Several nights from one read (q4 and q5 only): windows of 2 days ending
# at day 7 and 6 (q5's drop) and 5 (q4's, which reads as the PR's: q3 is not
# read), and the exit codes.
calls=$tmp/calls
fetch_statuses_real=$(declare -f fetch_statuses)
eval "${fetch_statuses_real/fetch_statuses ()/fetch_statuses_fake ()}"
fetch_statuses() {
  echo "$1" >>"$calls"
  fetch_statuses_fake "$1"
}
out=$(measure $((5 * day)) "$week" 3) && rc=0 || rc=$?
eq "$rc" 2
eq "$(grep -c . "$calls")" 2
eq "$(grep '^==' <<<"$out" | tr '\n' '|')" "== 1970-01-06T00:00:00Z to 1970-01-08T00:00:00Z ==|== 1970-01-05T00:00:00Z to 1970-01-07T00:00:00Z ==|== 1970-01-04T00:00:00Z to 1970-01-06T00:00:00Z ==|"
eq "$(tail -n 1 <<<"$out")" "missed a target in 2 of 3 windows"
out=$(measure $((5 * day)) "$week" 2) && rc=0 || rc=$?
eq "$rc" 3
eq "$(tail -n 1 <<<"$out")" "missed a target in 2 of 2 windows"
out=$(measure 0 "$day" 1) && rc=0 || rc=$?
eq "$rc" 0
eq "$(grep -c '^==' <<<"$out")" 0
out=$(measure 0 "$week" 1) && rc=0 || rc=$?
eq "$rc" 2
echo "ok: several nights read once, and their exit codes"

eq "$(bash scripts/ci-stats.sh --since 2026-01-01T00:00:00Z --nights 3 2>&1 >/dev/null)" "error: --nights takes --days, not --since"
eq "$(bash scripts/ci-stats.sh --nights 10 2>&1 >/dev/null)" "error: usage: scripts/ci-stats.sh [--days N | --since <UTC time>] [--nights K]"
echo "ok: arguments"

# scripts/ci-targets-issue.sh with a fake ci-stats (its exit from $tmp/rc)
# and a fake gh (the open issue's number in $tmp/open; every call logged).
mkdir -p "$tmp/bin"
cat >"$tmp/stats" <<EOF
echo "queue    FAIL  args: \$*"
exit \$(cat "$tmp/rc")
EOF
cat >"$tmp/bin/gh" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >>"$tmp/gh.log"
case "\$1 \$2" in
  "issue list") [ ! -s "$tmp/open" ] || cat "$tmp/open" ;;
  "issue close") : >"$tmp/open" ;;
  "issue create") echo 42 >"$tmp/open" ;;
esac
EOF
chmod +x "$tmp/bin/gh"
issue() { # ci-stats exit
  echo "$1" >"$tmp/rc"
  : >"$tmp/gh.log"
  PATH="$tmp/bin:$PATH" CI_STATS=$tmp/stats RUN_URL=https://run bash "$root/scripts/ci-targets-issue.sh" --days 7 --nights 3 >"$tmp/out" 2>&1
}
calls() { grep -c "^issue $1" "$tmp/gh.log"; }
: >"$tmp/open"
issue 0 || fail "a met night succeeds"
eq "$(calls close)" 0
issue 2 || fail "a missed night succeeds"
eq "$(calls create)$(calls comment)" 00
grep -q 'not yet every night; no issue' "$tmp/out" || fail "a missed night without an issue says so"
issue 3 || fail "the third missed night succeeds"
eq "$(calls create)" 1
grep -q 'queue    FAIL  args: --days 7 --nights 3' "$tmp/gh.log" || fail "the issue holds the report"
grep -q 'Run: https://run' "$tmp/gh.log" || fail "the issue links the run"
issue 2 || fail "a missed night with the issue open succeeds"
eq "$(calls comment)" 1
grep -q '^issue comment 42 ' "$tmp/gh.log" || fail "it comments on the open issue"
issue 3 || fail "another third night succeeds"
eq "$(calls create)$(calls comment)" 01
issue 0 || fail "a met night succeeds"
grep -q '^issue close 42 --comment Every target held: https://run\.' "$tmp/gh.log" || fail "a met night closes the issue"
if issue 1; then fail "a ci-stats error fails"; fi
eq "$(calls list)" 0
grep -q 'ci-stats.sh failed (exit 1)' "$tmp/out" || fail "an error says so"
echo "ok: the targets issue"

if [ "$failures" -gt 0 ]; then
  echo "error: $failures ci-stats check(s) failed" >&2
  exit 1
fi
echo "ci-stats: all checks passed"
