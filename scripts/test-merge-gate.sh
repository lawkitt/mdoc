#!/usr/bin/env bash
# Usage: scripts/test-merge-gate.sh
# scripts/merge-gate.sh (ADR 0095) against fake GitHub calls: its decisions,
# then one pass over PR heads and queue commits. No network. Bash 3.2
# compatible.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
MERGE_GATE_SOURCED=1
# shellcheck source=merge-gate.sh
source scripts/merge-gate.sh
log() { :; }

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

check() { # name command...
  local name=$1
  shift
  "$@" || fail "$name"
  echo "ok: $name"
}

eq() { [ "$1" = "$2" ] || fail "expected '$2', got '$1'"; }

# --- decisions -------------------------------------------------------------------

eq "$(printf 'docs/adr/0095-ci-on-a-merge-queue.md\nREADME.md\n' | areas_of)" ""
eq "$(printf 'apps/ios/mdoc/App.swift\n' | areas_of)" "ios"
eq "$(printf 'crates/mdoc-core/src/lib.rs\n' | areas_of)" "core desktop recognition ios android"
# Recognition code is no core change: the app jobs skip it (#526).
eq "$(printf 'crates/mdoc-pii/src/lib.rs\n' | areas_of)" "desktop recognition"
eq "$(printf 'tests/fixtures/recognition/en/letter/x.md\n' | areas_of)" "recognition"
echo "ok: areas from a file list"

eq "$(needed_jobs "core desktop recognition ios android")" "macos windows ios android detection"
eq "$(needed_jobs "desktop")" "macos windows detection"
eq "$(needed_jobs "ios")" "ios"
eq "$(needed_jobs "recognition")" "detection"
eq "$(needed_jobs "")" ""
echo "ok: jobs from areas"

# queue.yml's areas job: what a commit brings onto origin/main's merge base.
repo_dir="$tmp/areas"
mkdir -p "$repo_dir/scripts"
cp scripts/ci-areas.sh scripts/ci-areas.txt scripts/lib.sh "$repo_dir/scripts/"
(
  cd "$repo_dir" || exit 1
  g() { git -c user.name=t -c user.email=t@t -c commit.gpgsign=false "$@" >/dev/null; }
  g init -q
  g add -A && g commit -qm base
  g update-ref refs/remotes/origin/main HEAD
  mkdir -p apps/ios && echo x >apps/ios/App.swift
  g add -A && g commit -qm "iOS (#1)"
  eq "$(bash scripts/ci-areas.sh --queue | tr '\n' ' ')" \
    "areas=ios jobs=ios macos=false ios=true detection=false "
  mkdir -p crates/mdoc-pii && echo x >crates/mdoc-pii/lib.rs
  g add -A && g commit -qm "PII (#2)"
  eq "$(bash scripts/ci-areas.sh --queue | tr '\n' ' ')" \
    "areas=desktop recognition ios jobs=macos windows ios detection macos=true ios=true detection=true "
  # No main to diff against: every job.
  g update-ref -d refs/remotes/origin/main
  eq "$(bash scripts/ci-areas.sh --queue | sed -n 2p)" "jobs=macos windows ios android detection"
) || exit 1
for job in macos ios detection; do
  grep -q "if: needs.areas.outputs.$job == 'true'" .github/workflows/queue.yml || fail "queue.yml starts no $job job"
done
grep -q 'run: scripts/ci-areas.sh --queue' .github/workflows/queue.yml || fail "queue.yml's areas job reads no areas"
echo "ok: queue.yml's jobs are the ones merge-gate waits for"

eq "$(head_gate "" missing)" "success|Docs only: no job runs"
eq "$(head_gate "desktop" "success 1")" "success|Fast tier passed (unit, scripts); the full suite runs in the merge queue"
eq "$(head_gate "desktop" "failure https://x")" "failure|Fast tier failed: fast (ubuntu)"
eq "$(head_gate "desktop" missing)" "pending|Waiting for the fast tier: fast (ubuntu)"
echo "ok: a PR head's gate"

runs='{"check_runs": [
  {"id": 1, "name": "queue / ci/macos", "status": "completed", "conclusion": "failure", "html_url": "https://old", "completed_at": "2026-11-02T11:00:00Z"},
  {"id": 5, "name": "queue / ci/macos", "status": "completed", "conclusion": "success", "html_url": "https://mac", "completed_at": "2026-11-02T11:30:00Z"},
  {"id": 2, "name": "detection / ci/detection (1)", "status": "completed", "conclusion": "success", "html_url": "https://d1", "completed_at": "2026-11-02T11:20:00Z"},
  {"id": 3, "name": "detection / ci/detection", "status": "completed", "conclusion": "success", "html_url": "https://d", "completed_at": "2026-11-02T11:21:00Z"},
  {"id": 4, "name": "ios / ci/ios", "status": "in_progress", "conclusion": null, "html_url": "https://ios"},
  {"id": 6, "name": "ci/windows (hosted)", "status": "completed", "conclusion": "skipped", "html_url": "https://w"},
  {"id": 7, "name": "fast (ubuntu)", "status": "completed", "conclusion": "timed_out", "html_url": "https://fast", "completed_at": "2026-11-02T11:05:00Z"}
]}'
eq "$(check_verdict ci/macos <<<"$runs")" "success https://mac 1793619000"
eq "$(check_verdict ci/detection <<<"$runs")" "success https://d 1793618460"
eq "$(check_verdict ci/ios <<<"$runs")" "pending - 0"
eq "$(check_verdict "ci/windows (hosted)" <<<"$runs")" "skipped - 0"
eq "$(check_verdict "fast (ubuntu)" <<<"$runs")" "failure https://fast 1793617500"
eq "$(check_verdict ci/android <<<"$runs")" "missing - 0"
for c in stale neutral cancelled action_required; do
  one=$(printf '{"check_runs": [{"id": 1, "name": "ci/ios", "status": "completed", "conclusion": "%s", "html_url": "https://i"}]}' "$c")
  eq "$(check_verdict ci/ios <<<"$one")" "failure https://i 0"
done
# A shard alone is not the gate: only the summary job counts.
eq "$(check_verdict ci/detection <<<'{"check_runs": [{"id": 2, "name": "ci/detection (1)", "status": "completed", "conclusion": "success", "html_url": "https://d1"}]}')" "missing - 0"
echo "ok: check-run verdicts (newest run, called jobs, only success passes)"

statuses='{"statuses": [
  {"context": "ci/windows", "state": "pending", "updated_at": "2026-11-02T12:00:00Z", "target_url": "https://pc/w"},
  {"context": "merge-gate", "state": "pending", "updated_at": "2026-11-02T12:00:00Z", "description": "Waiting: windows"}]}'
eq "$(status_of ci/windows <<<"$statuses")" "pending 1793620800 https://pc/w"
eq "$(status_of ci/android <<<"$statuses")" "none 0 -"
echo "ok: status lines"

t=1793620800 # 2026-11-02T12:00:00Z
eq "$(pc_job "success $t https://pc" "missing - 0" 0 false 30 "$t")" "success PC https://pc"
eq "$(pc_job "pending $t https://pc" "success https://h $t" 1 false 30 "$t")" "success hosted https://h"
eq "$(pc_job "failure $t https://pc" "pending - 0" 1 false 30 "$t")" "failure PC https://pc"
eq "$(pc_job "none 0 -" "failure https://h $t" 1 false 30 "$t")" "failure hosted https://h"
# Both finished: the earlier result counts, either way round.
eq "$(pc_job "failure $((t - 60)) https://pc" "success https://h $t" 1 false 30 "$t")" "failure PC https://pc"
eq "$(pc_job "success $t https://pc" "failure https://h $((t - 60))" 1 false 30 "$t")" "failure hosted https://h"
eq "$(pc_job "success $((t - 60)) https://pc" "failure https://h $t" 1 false 30 "$t")" "success PC https://pc"
eq "$(pc_job "none 0 -" "missing - 0" 0 true 0 "$t")" "fallback hosted third-party"
eq "$(pc_job "error $t https://pc" "missing - 0" 0 false 30 "$t")" "fallback hosted pc-error"
eq "$(pc_job "none 0 -" "missing - 0" 0 false 9 "$t")" "pending PC -"
eq "$(pc_job "none 0 -" "missing - 0" 0 false 10 "$t")" "fallback hosted silent"
eq "$(pc_job "pending $((t - 540)) https://pc" "missing - 0" 0 false 30 "$t")" "pending PC https://pc"
eq "$(pc_job "pending $((t - 600)) https://pc" "missing - 0" 0 false 30 "$t")" "fallback hosted stale"
eq "$(pc_job "none 0 -" "missing - 0" 1 false 30 "$t")" "pending hosted -"
echo "ok: PC jobs: the first result counts, fallback when silent, stale, erroring or third-party"

eq "$(printf 'Web address category (#514)\nMerge pull request #77 from x/y\nfix: no number\n' | group_prs | tr '\n' ' ')" "514 77 "
echo "ok: PR numbers from commit subjects"

eq "$(printf 'macos success hosted -\nwindows success PC https://pc\ndetection success hosted -\n' | queue_gate https://run)" \
  "success|Passed: macos (hosted), windows (PC), detection (hosted)|https://run"
eq "$(printf 'macos success hosted -\nios skipped - -\n' | queue_gate https://run)" \
  "success|Passed: macos (hosted); not affected: ios|https://run"
eq "$(printf 'macos success hosted -\nwindows failure PC https://pc\nios pending - -\n' | queue_gate https://run)" \
  "failure|Failed: windows (PC); passed: macos (hosted)|https://pc"
eq "$(printf 'macos success hosted -\nandroid pending hosted -\n' | queue_gate https://run)" \
  "pending|Waiting: android; passed: macos (hosted)|https://run"
eq "$(printf '' | queue_gate https://run)" "success|Docs only: no job runs|https://run"
echo "ok: a queue commit's gate"

# --- one pass against fakes --------------------------------------------------

posted="$tmp/posted" dispatched="$tmp/dispatched"
: >"$posted"
: >"$dispatched"
now() { echo 1793620800; }
fetch_heads() { printf '1 false aaaa\n2 false bbbb\n3 true cccc\n4 false dddd\n5 false eeee\n'; }
fetch_pr_files() {
  case "$1" in
    1) echo docs/x.md ;;
    # A rename out of the iOS app into docs: both paths count.
    5) printf 'docs/App.swift.md\napps/ios/mdoc/App.swift\n' ;;
    *) echo src/main.rs ;;
  esac
}
fetch_check_runs() {
  case "$1" in
    bbbb) echo '{"check_runs": [{"id": 1, "name": "fast (ubuntu)", "status": "completed", "conclusion": "success", "html_url": "https://f"}]}' ;;
    qqqq) echo "$runs" ;;
    *) echo '{"check_runs": []}' ;;
  esac
}
fetch_status() {
  case "$1" in
    dddd) echo '{"statuses": [{"context": "merge-gate", "state": "pending", "description": "Waiting for the fast tier: fast (ubuntu)"}]}' ;;
    qqqq) echo '{"statuses": [{"context": "ci/windows", "state": "success", "updated_at": "2026-11-02T11:50:00Z", "target_url": "https://pc/w"}]}' ;;
    *) echo '{"statuses": []}' ;;
  esac
}
post() { echo "$1 $2 $3" >>"$posted"; }
fetch_main() { echo mmmm; }
fetch_queue() { echo "gh-readonly-queue/main/pr-514-mmmm qqqq"; }
fetch_compare() {
  # The \n escapes are JSON's, for jq.
  # shellcheck disable=SC2028
  echo '{"files": [{"filename": "docs/lock.md", "previous_filename": "Cargo.lock"}], "commits": [{"commit": {"message": "Web address category (#514)\n\nbody"}}]}'
}
fetch_commit_time() { echo $((1793620800 - 720)); }
fetch_pr() { echo "User false"; }
fetch_dispatched() { echo 0; }
dispatch() { echo "$*" >>"$dispatched"; }
fetch_workflow_state() { echo active; }

heads_pass
eq "$(sort "$posted")" "aaaa success Docs only: no job runs
bbbb success Fast tier passed (unit, scripts); the full suite runs in the merge queue
eeee pending Waiting for the fast tier: fast (ubuntu)"
echo "ok: heads: docs only, fast tier passed, a rename counts both paths, drafts and unchanged statuses left alone"

: >"$posted"
queue_pass
# Cargo.lock (renamed away) is core: every job. macos passed, ios runs, detection passed,
# windows passed on the PC, android silent for 12 minutes: dispatched hosted.
eq "$(cat "$dispatched")" "ci-android.yml gh-readonly-queue/main/pr-514-mmmm qqqq"
eq "$(cat "$posted")" "qqqq pending Waiting: ios, android; passed: macos (hosted), windows (PC), detection (hosted)"
echo "ok: queue: jobs by area, a silent PC job falls back, the gate waits"

# A bot PR in the group: Windows and Android go hosted at once, even with the
# PC quick (the commit is 1 minute old).
: >"$posted"
: >"$dispatched"
fetch_pr() { echo "Bot false"; }
fetch_commit_time() { echo $((1793620800 - 60)); }
fetch_status() { echo '{"statuses": []}'; }
queue_pass
eq "$(cat "$dispatched")" "ci-desktop.yml gh-readonly-queue/main/pr-514-mmmm qqqq os=windows
ci-android.yml gh-readonly-queue/main/pr-514-mmmm qqqq"
echo "ok: queue: a fork or bot PR never runs on the PC"

# No hosted Android job yet (ci-android.yml is a 404): the Android job fails
# at once, saying why, when it has nowhere to run; hosted Windows still runs.
fetch_workflow_state() {
  case "$1" in
    ci-android.yml)
      echo "gh: Not Found (HTTP 404)" >&2
      return 1
      ;;
    *) echo active ;;
  esac
}
: >"$posted"
: >"$dispatched"
queue_pass
eq "$(cat "$dispatched")" "ci-desktop.yml gh-readonly-queue/main/pr-514-mmmm qqqq os=windows"
eq "$(cat "$posted")" "qqqq failure Failed: android (no hosted job; third-party code never runs on the PC); passed: macos (hosted), detection (hosted)"
# A PC that has posted ci/android nowhere for 12 minutes: gone.
fetch_pr() { echo "User false"; }
fetch_commit_time() { echo $((1793620800 - 720)); }
fetch_status() {
  case "$1" in
    qqqq) echo '{"statuses": [{"context": "ci/windows", "state": "success", "updated_at": "2026-11-02T11:50:00Z", "target_url": "https://pc/w"}]}' ;;
    mmmm) echo '{"statuses": [{"context": "ci/android", "state": "success", "updated_at": "2026-11-02T11:40:00Z"}]}' ;;
    *) echo '{"statuses": []}' ;;
  esac
}
: >"$posted"
: >"$dispatched"
queue_pass
eq "$(cat "$dispatched")" ""
eq "$(cat "$posted")" "qqqq failure Failed: android (no hosted job; PC silent 10 min); passed: macos (hosted), windows (PC), detection (hosted)"
# The PC busy with an earlier group (a fresh pending ci/android there): wait.
fetch_queue() { printf '%s\n' "gh-readonly-queue/main/pr-513-mmmm pppp" "gh-readonly-queue/main/pr-514-pppp qqqq"; }
fetch_status() {
  case "$1" in
    qqqq) echo '{"statuses": [{"context": "ci/windows", "state": "success", "updated_at": "2026-11-02T11:50:00Z", "target_url": "https://pc/w"}]}' ;;
    pppp) echo '{"statuses": [{"context": "ci/android", "state": "pending", "updated_at": "2026-11-02T11:58:00Z"}]}' ;;
    *) echo '{"statuses": []}' ;;
  esac
}
: >"$posted"
queue_pass
eq "$(grep '^qqqq' "$posted")" "qqqq pending Waiting: ios, android; passed: macos (hosted), windows (PC), detection (hosted)"
# Its runner erred, or its heartbeat stopped: nowhere to run.
for s in "error 2026-11-02T11:58:00Z" "pending 2026-11-02T11:45:00Z"; do
  fetch_status() { printf '{"statuses": [{"context": "ci/android", "state": "%s", "updated_at": "%s"}]}\n' "${s% *}" "${s#* }"; }
  : >"$posted"
  queue_pass
  case "$s" in error*) why="PC runner error" ;; *) why="PC heartbeat stopped" ;; esac
  eq "$(grep '^qqqq' "$posted" | sed 's/;  *passed.*//; s/)$//')" "qqqq failure Failed: android (no hosted job; $why"
done
# GitHub could not say whether the workflow exists: wait for the next pass.
fetch_workflow_state() {
  echo "error connecting to api.github.com" >&2
  return 1
}
fetch_status() { echo '{"statuses": []}'; }
: >"$posted"
: >"$dispatched"
queue_pass
eq "$(grep -c failure "$posted")" 0
# A disabled workflow is no fallback either.
fetch_workflow_state() { echo disabled_manually; }
: >"$posted"
queue_pass
eq "$(grep '^qqqq' "$posted" | sed 's/;  *passed.*//')" "qqqq failure Failed: windows (ci-desktop.yml is disabled manually; PC silent 10 min), android (ci-android.yml is disabled manually; PC silent 10 min)"
echo "ok: queue: with no hosted job a PC job that cannot run fails at once, saying why"

# Runs of queue commits the queue dropped are cancelled; current ones,
# other branches and PR runs are not.
cancelled="$tmp/cancelled"
: >"$cancelled"
fetch_queue() { echo "gh-readonly-queue/main/pr-514-mmmm qqqq"; }
fetch_active_runs() {
  printf '%s\n' "11 gh-readonly-queue/main/pr-514-mmmm" "12 gh-readonly-queue/main/pr-513-oldbase" \
    "13 main" "14 mac/topic" "15 ci-shadow/main/gone" "16 gh-readonly-queue/main/pr-513-oldbase"
}
cancel_run() { echo "$1" >>"$cancelled"; }
stale_pass
eq "$(tr '\n' ' ' <"$cancelled")" "12 15 16 "
echo "ok: runs of queue commits that left the queue are cancelled"

echo "merge gate: all checks passed"
