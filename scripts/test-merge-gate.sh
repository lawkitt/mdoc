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
eq "$(printf 'crates/mdoc-pii/src/lib.rs\n' | areas_of)" "core desktop recognition ios android"
eq "$(printf 'tests/fixtures/recognition/en/letter/x.md\n' | areas_of)" "recognition"
echo "ok: areas from a file list"

eq "$(needed_jobs "core desktop recognition ios android")" "macos windows ios android detection"
eq "$(needed_jobs "desktop")" "macos windows detection"
eq "$(needed_jobs "ios")" "ios"
eq "$(needed_jobs "recognition")" "detection"
eq "$(needed_jobs "")" ""
echo "ok: jobs from areas"

eq "$(head_gate "" missing)" "success|Docs only: no job runs"
eq "$(head_gate "desktop" "success 1")" "success|Fast tier passed (unit, scripts); the full suite runs in the merge queue"
eq "$(head_gate "desktop" "failure https://x")" "failure|Fast tier failed: fast (ubuntu)"
eq "$(head_gate "desktop" missing)" "pending|Waiting for the fast tier: fast (ubuntu)"
echo "ok: a PR head's gate"

runs='{"check_runs": [
  {"id": 1, "name": "queue / ci/macos", "status": "completed", "conclusion": "failure", "html_url": "https://old"},
  {"id": 5, "name": "queue / ci/macos", "status": "completed", "conclusion": "success", "html_url": "https://mac"},
  {"id": 2, "name": "detection / ci/detection (1)", "status": "completed", "conclusion": "success", "html_url": "https://d1"},
  {"id": 3, "name": "detection / ci/detection (2)", "status": "completed", "conclusion": "success", "html_url": "https://d2"},
  {"id": 4, "name": "ios / ci/ios", "status": "in_progress", "conclusion": null, "html_url": "https://ios"},
  {"id": 6, "name": "ci/windows (hosted)", "status": "completed", "conclusion": "skipped", "html_url": "https://w"},
  {"id": 7, "name": "fast (ubuntu)", "status": "completed", "conclusion": "timed_out", "html_url": "https://fast"}
]}'
eq "$(check_verdict ci/macos <<<"$runs")" "success 1"
eq "$(check_verdict "ci/detection (" <<<"$runs")" "success 2"
eq "$(check_verdict ci/ios <<<"$runs")" "pending"
eq "$(check_verdict "ci/windows (hosted)" <<<"$runs")" "skipped"
eq "$(check_verdict "fast (ubuntu)" <<<"$runs")" "failure https://fast"
eq "$(check_verdict ci/android <<<"$runs")" "missing"
shard_fail='{"check_runs": [
  {"id": 2, "name": "ci/detection (1)", "status": "completed", "conclusion": "success", "html_url": "https://d1"},
  {"id": 3, "name": "ci/detection (2)", "status": "completed", "conclusion": "failure", "html_url": "https://d2"}]}'
eq "$(check_verdict "ci/detection (" <<<"$shard_fail")" "failure https://d2"
echo "ok: check-run verdicts (newest run, called jobs, shards)"

statuses='{"statuses": [
  {"context": "ci/windows", "state": "pending", "updated_at": "2026-11-02T12:00:00Z", "target_url": "https://pc/w"},
  {"context": "merge-gate", "state": "pending", "updated_at": "2026-11-02T12:00:00Z", "description": "Waiting: windows"}]}'
eq "$(status_of ci/windows <<<"$statuses")" "pending 1793620800 https://pc/w"
eq "$(status_of ci/android <<<"$statuses")" "none 0 -"
echo "ok: status lines"

t=1793620800 # 2026-11-02T12:00:00Z
eq "$(pc_job "success $t https://pc" missing 0 false 30 "$t")" "success PC https://pc"
eq "$(pc_job "pending $t https://pc" "success 1" 1 false 30 "$t")" "success hosted -"
eq "$(pc_job "failure $t https://pc" pending 1 false 30 "$t")" "failure PC https://pc"
eq "$(pc_job "none 0 -" "failure https://h" 1 false 30 "$t")" "failure hosted https://h"
eq "$(pc_job "none 0 -" missing 0 true 0 "$t")" "fallback hosted third-party"
eq "$(pc_job "none 0 -" missing 0 false 9 "$t")" "pending PC -"
eq "$(pc_job "none 0 -" missing 0 false 10 "$t")" "fallback hosted silent"
eq "$(pc_job "pending $((t - 540)) https://pc" missing 0 false 30 "$t")" "pending PC https://pc"
eq "$(pc_job "pending $((t - 600)) https://pc" missing 0 false 30 "$t")" "fallback hosted stale"
eq "$(pc_job "none 0 -" missing 1 false 30 "$t")" "pending hosted -"
echo "ok: PC jobs: first result counts, fallback when silent, stale or third-party"

eq "$(printf 'Web address category (#514)\nMerge pull request #77 from x/y\nfix: no number\n' | group_prs | tr '\n' ' ')" "514 77 "
echo "ok: PR numbers from commit subjects"

eq "$(printf 'macos success hosted -\nwindows success PC https://pc\ndetection success 2_shards -\n' | queue_gate https://run)" \
  "success|Passed: macos (hosted), windows (PC), detection (2 shards)|https://run"
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
fetch_heads() {
  cat <<'JSON'
{"data": {"repository": {"pullRequests": {"nodes": [
  {"number": 1, "isDraft": false, "headRefOid": "aaaa", "files": {"totalCount": 1, "nodes": [{"path": "docs/x.md"}]}},
  {"number": 2, "isDraft": false, "headRefOid": "bbbb", "files": {"totalCount": 1, "nodes": [{"path": "src/main.rs"}]}},
  {"number": 3, "isDraft": true, "headRefOid": "cccc", "files": {"totalCount": 1, "nodes": [{"path": "src/main.rs"}]}},
  {"number": 4, "isDraft": false, "headRefOid": "dddd", "files": {"totalCount": 1, "nodes": [{"path": "src/main.rs"}]}}
]}}}}
JSON
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
  echo '{"files": [{"filename": "Cargo.lock"}], "commits": [{"commit": {"message": "Web address category (#514)\n\nbody"}}]}'
}
fetch_commit_time() { echo $((1793620800 - 720)); }
fetch_pr() { echo "User false"; }
fetch_dispatched() { echo 0; }
dispatch() { echo "$*" >>"$dispatched"; }

heads_pass
eq "$(sort "$posted")" "aaaa success Docs only: no job runs
bbbb success Fast tier passed (unit, scripts); the full suite runs in the merge queue"
echo "ok: heads: docs only, fast tier passed, drafts and unchanged statuses left alone"

: >"$posted"
queue_pass
# Cargo.lock is core: every job. macos passed, ios runs, detection passed,
# windows passed on the PC, android silent for 12 minutes: dispatched hosted.
eq "$(cat "$dispatched")" "ci-android.yml gh-readonly-queue/main/pr-514-mmmm qqqq"
eq "$(cat "$posted")" "qqqq pending Waiting: ios, android; passed: macos (hosted), windows (PC), detection (2 shards)"
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

echo "merge gate: all checks passed"
