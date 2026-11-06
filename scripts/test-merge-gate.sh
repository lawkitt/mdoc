#!/usr/bin/env bash
# Usage: scripts/test-merge-gate.sh
# scripts/merge-gate.sh (ADR 0095, ADR 0100) against fake GitHub calls: its
# GitHub wrapper, its decisions, then passes over PR heads and queue commits.
# No network. Bash 3.2 compatible.
# shellcheck disable=SC2317,SC2329 # the fakes are called by the sourced gate
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
MERGE_GATE_SOURCED=1
MERGE_GATE_TMP="$tmp/gate"
# shellcheck source=merge-gate.sh
source scripts/merge-gate.sh
logf="$tmp/log"
log() { echo "$*" >>"$logf"; }
retry_delay=0

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

# --- the GitHub wrapper (a fake gh) ------------------------------------------

calls="$tmp/calls"
reset() {
  : >"$calls"
  : >"$logf"
  : >"$gate_tmp/errors"
}
ncalls() { wc -l <"$calls" | tr -d ' '; }

eq "$(gh_what api graphql --paginate -f owner=o)" "POST graphql"
eq "$(gh_what api -X POST repos/o/r/statuses/abc -f state=success)" "POST repos/o/r/statuses/abc"
eq "$(gh_what api --paginate "repos/o/r/commits/abc/check-runs?per_page=100" --jq .x)" "GET repos/o/r/commits/abc/check-runs?per_page=100"
eq "$(gh_what workflow run merge-gate-recheck.yml --repo o/r --ref main)" "workflow run merge-gate-recheck.yml"
echo "ok: a call names its endpoint"

# A truncated body once, then a whole one: retried, and no error.
reset
gh() {
  echo "$*" >>"$calls"
  if [ "$(ncalls)" -lt 2 ]; then printf '{"statuses": ['; else echo '{"statuses": []}'; fi
}
eq "$(fetch_status abc)" '{"statuses": []}'
eq "$(ncalls)" 2
eq "$(cat "$gate_tmp/errors")" ""
# gh's own "unexpected end of JSON input" (a truncated page under --jq, run
# 42060750092) every time: 3 tries, then the call fails, naming its endpoint,
# and the pass's error list has it.
reset
gh() {
  echo "$*" >>"$calls"
  echo "unexpected end of JSON input" >&2
  return 1
}
if fetch_check_runs abc >/dev/null; then fail "a failed call returned success"; fi
eq "$(ncalls)" 3
eq "$(cat "$logf")" "GitHub call failed (3 tries): GET repos/$repo/commits/abc/check-runs?per_page=100: unexpected end of JSON input"
eq "$(cat "$gate_tmp/errors")" "GET repos/$repo/commits/abc/check-runs?per_page=100"
# HTTP 500 and then a good answer.
reset
gh() {
  echo "$*" >>"$calls"
  if [ "$(ncalls)" -lt 3 ]; then
    echo "gh: Server Error (HTTP 502)" >&2
    return 1
  fi
  echo abc123
}
eq "$(fetch_main)" abc123
eq "$(cat "$gate_tmp/errors")" ""
# HTTP 404 is an answer: one try, gh's message for the caller, no error.
reset
gh() {
  echo "$*" >>"$calls"
  echo "gh: Not Found (HTTP 404)" >&2
  return 1
}
if out=$(fetch_workflow_state ci-android.yml 2>&1); then fail "a 404 returned success"; fi
eq "$out" "gh: Not Found (HTTP 404)"
eq "$(ncalls)" 1
eq "$(cat "$gate_tmp/errors")$(cat "$logf")" ""
# No such workflow when counting its manual runs (ci-android.yml not on
# main): 0 runs, not an error that skips the queue commit (the first passes
# after #619 failed every queue commit that needed ci/android).
eq "$(fetch_dispatched ci-android.yml abc 2>&1)" 0
eq "$(cat "$gate_tmp/errors")$(cat "$logf")" ""
# A run that finished before its cancel (HTTP 409): no "cannot cancel" noise.
reset
gh() {
  echo "gh: Cannot cancel a workflow run that is completed. (HTTP 409)" >&2
  return 1
}
if cancel_run 5 2>"$tmp/stderr"; then fail "a 409 cancel returned success"; fi
eq "$(cat "$tmp/stderr")$(cat "$logf")$(cat "$gate_tmp/errors")" ""
echo "ok: GitHub calls retry with backoff, fail naming the endpoint, take 404 and 409 as answers"

# A call that hangs is cut off at its limit (needs timeout(1), as on ubuntu).
if [ -n "$timeout_cmd" ]; then
  mkdir -p "$tmp/bin"
  printf '#!/bin/sh\nsleep 5\n' >"$tmp/bin/gh"
  chmod +x "$tmp/bin/gh"
  unset -f gh
  reset
  saved_path=$PATH
  PATH="$tmp/bin:$PATH" tries=1 call_timeout=1
  if fetch_main >/dev/null; then fail "a hung call returned success"; fi
  PATH=$saved_path tries=3 call_timeout=30
  eq "$(cat "$logf")" "GitHub call failed (1 tries): GET repos/$repo/commits/main: no answer in 1s"
  echo "ok: a hung call is cut off"
else
  echo "skip: a hung call is cut off (no timeout command)"
fi

# The PR list: auto-merge and the queue read from GraphQL.
reset
gh() {
  echo '{"data": {"repository": {"pullRequests": {"nodes": [
    {"number": 7, "isDraft": false, "headRefOid": "h7", "isInMergeQueue": false, "autoMergeRequest": {"enabledAt": "2026-11-02T11:50:00Z"}},
    {"number": 8, "isDraft": true, "headRefOid": "h8", "isInMergeQueue": true, "autoMergeRequest": null}]}}}}'
}
eq "$(fetch_heads)" "7 false h7 1793620200 false
8 true h8 0 true"
echo "ok: PR lines with auto-merge and queue state"

# From here on every GitHub call is faked one level up; a real one is a bug.
gh() {
  echo "gh $*" >>"$tmp/unexpected-gh"
  return 1
}

# --- decisions -------------------------------------------------------------------

eq "$(printf 'docs/adr/0095-ci-on-a-merge-queue.md\nREADME.md\n' | areas_of)" ""
eq "$(printf 'apps/ios/mdoc/App.swift\n' | areas_of)" "ios"
eq "$(printf 'crates/mdoc-core/src/lib.rs\n' | areas_of)" "core desktop recognition ios android ci"
# Recognition code is no core change: the app jobs skip it (#526).
eq "$(printf 'crates/mdoc-pii/src/lib.rs\n' | areas_of)" "desktop recognition"
eq "$(printf 'tests/fixtures/recognition/en/letter/x.md\n' | areas_of)" "recognition"
# The CI machinery (ADR 0100): the script suites, not the desktop jobs.
eq "$(printf 'scripts/merge-gate.sh\n.github/workflows/ci.yml\n' | areas_of)" "ci"
eq "$(printf 'scripts/merge-gate.sh\nsrc/main.rs\n' | areas_of)" "desktop ci"
echo "ok: areas from a file list"

eq "$(needed_jobs "core desktop recognition ios android ci")" "macos windows ios android detection"
eq "$(needed_jobs "desktop")" "macos windows detection"
eq "$(needed_jobs "ios")" "ios"
eq "$(needed_jobs "recognition")" "detection"
eq "$(needed_jobs "ci")" "scripts"
# ci/macos runs the suites too.
eq "$(needed_jobs "desktop ci")" "macos windows detection"
eq "$(needed_jobs "ios ci")" "ios scripts"
eq "$(needed_jobs "recognition ci")" "detection scripts"
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
    "areas=ios jobs=ios macos=false ios=true detection=false scripts=false "
  mkdir -p crates/mdoc-pii && echo x >crates/mdoc-pii/lib.rs
  g add -A && g commit -qm "PII (#2)"
  eq "$(bash scripts/ci-areas.sh --queue | tr '\n' ' ')" \
    "areas=desktop recognition ios jobs=macos windows ios detection macos=true ios=true detection=true scripts=false "
  # The CI machinery alone (ADR 0100): only the script suites.
  g update-ref refs/remotes/origin/main HEAD
  echo x >scripts/merge-gate.sh
  mkdir -p .github/workflows && echo x >.github/workflows/merge-gate.yml
  g add -A && g commit -qm "Gate (#3)"
  eq "$(bash scripts/ci-areas.sh --queue | tr '\n' ' ')" \
    "areas=ci jobs=scripts macos=false ios=false detection=false scripts=true "
  # No main to diff against: every job.
  g update-ref -d refs/remotes/origin/main
  eq "$(bash scripts/ci-areas.sh --queue | sed -n 2p)" "jobs=macos windows ios android detection"
) || exit 1
for job in macos ios detection scripts; do
  grep -q "if: needs.areas.outputs.$job == 'true'" .github/workflows/queue.yml || fail "queue.yml starts no $job job"
done
grep -q 'run: scripts/ci-areas.sh --queue' .github/workflows/queue.yml || fail "queue.yml's areas job reads no areas"
echo "ok: queue.yml's jobs are the ones merge-gate waits for"

# Detection on PR heads (ADR 0100): only a file on a recognition line of the
# table, never core's every area; ci.yml's areas job says the same.
detects() { if tr '\n' '\0' | head_detection "$(cat scripts/ci-areas.txt)"; then echo yes; else echo no; fi; }
eq "$(printf 'tests/fixtures/recognition/en/letter/x.model.json\n' | detects)" yes
eq "$(printf 'crates/mdoc-pii/src/lib.rs\n' | detects)" yes
eq "$(printf 'crates/mdoc-detect/src/lib.rs\nsrc/main.rs\n' | detects)" yes
eq "$(printf 'crates/mdoc-core/src/lib.rs\ncrates/mdoc-detect/src/lib.rs\n' | detects)" yes
eq "$(printf 'crates/mdoc-core/src/lib.rs\n' | detects)" no
eq "$(printf 'Cargo.lock\nscripts/lib.sh\n' | detects)" no
eq "$(printf 'src/main.rs\n' | detects)" no
eq "$(printf 'docs/x.md\n' | detects)" no
eq "$(printf '' | detects)" no
eq "$(printf 'tests/fixtures/recognition/x.md\0' | head_detection "$(sed 's/$/\r/' scripts/ci-areas.txt)" && echo yes)" yes
(
  cd "$repo_dir" || exit 1
  g() { git -c user.name=t -c user.email=t@t -c commit.gpgsign=false "$@" >/dev/null; }
  mkdir -p tests/fixtures/recognition && echo x >tests/fixtures/recognition/x.model.json
  g add -A && g commit -qm fixture
  eq "$(EVENT=pull_request bash scripts/ci-areas.sh --github | tr '\n' ' ')" "areas=recognition desktop=true detection-head=true "
  echo y >Cargo.toml
  g add -A && g commit -qm core
  eq "$(EVENT=pull_request bash scripts/ci-areas.sh --github | sed -n 3p)" "detection-head=false"
  # Off a PR, no such output.
  eq "$(EVENT=workflow_dispatch bash scripts/ci-areas.sh --github | grep -c detection-head)" 0
) || exit 1
grep -q "detection-head: \${{ steps.areas.outputs.detection-head }}" .github/workflows/ci.yml || fail "ci.yml's areas job has no detection-head output"
grep -q "if: needs.areas.outputs.detection-head == 'true'" .github/workflows/ci.yml || fail "ci.yml starts no head detection"
grep -qx "  $head_detection_job:" .github/workflows/ci.yml || fail "ci.yml's head detection is not $head_detection_job"
echo "ok: recognition-area PR heads run the detection gate"

eq "$(head_gate "" missing)" "success|Docs only: no job runs"
eq "$(head_gate "desktop" "success 1")" "success|Fast tier passed (unit, scripts); the full suite runs in the merge queue"
eq "$(head_gate "desktop" "failure https://x")" "failure|Fast tier failed: fast (ubuntu)"
eq "$(head_gate "desktop" missing)" "pending|Waiting for the fast tier: fast (ubuntu)"
eq "$(head_gate "desktop" "infra no_Linux_runner,_3_attempts")" "failure|Fast tier did not run (no Linux runner, 3 attempts): fast (ubuntu)"
# A recognition-area PR also waits for the detection gate on its head.
eq "$(head_gate "recognition" "success 1" "success 2" 575)" "success|Fast tier and detection gate passed; the full suite runs in the merge queue"
eq "$(head_gate "recognition" "success 1" "pending - 0" 575)" "pending|Fast tier passed; waiting for the detection gate: detection-head / ci/detection"
eq "$(head_gate "recognition" "pending - 0" "missing - 0" 575)" "pending|Waiting for the fast tier and the detection gate: fast (ubuntu), detection-head / ci/detection"
eq "$(head_gate "recognition" "missing - 0" "success 2" 575)" "pending|Detection gate passed; waiting for the fast tier: fast (ubuntu)"
eq "$(head_gate "recognition" "success 1" "failure https://d 2" 575)" "failure|Detection gate failed: detection-head / ci/detection; regenerated snapshots: just snapshots-fetch 575"
eq "$(head_gate "desktop recognition" "failure https://f 1" "failure https://d 2" 575)" "failure|Failed: fast (ubuntu); detection-head / ci/detection (just snapshots-fetch 575)"
eq "$(head_gate "recognition" "success 1" "infra no_macOS_runner,_3_attempts" 575)" "failure|Detection gate did not run (no macOS runner, 3 attempts): detection-head / ci/detection"
eq "$(head_gate "recognition" "pending - 0" "failure https://d 2" 575)" "failure|Detection gate failed: detection-head / ci/detection; regenerated snapshots: just snapshots-fetch 575"
# Skipped on the head (the PR's own table said no): the fast tier alone.
eq "$(head_gate "recognition" "success 1" "skipped - 0" 575)" "success|Fast tier passed (unit, scripts); the full suite runs in the merge queue"
long=$(head_gate "desktop recognition" "infra no_Linux_runner,_3_attempts" "infra no_macOS_runner,_3_attempts" 99999)
[ "${#long}" -le 148 ] || fail "a head gate description over 140 characters: $long"
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

# Jobs that never ran: run 41857275799 (2026-11-04), its macOS jobs not
# acquired by a runner, and the detection summary failing over its shards.
never='{"id": 0, "conclusion": "cancelled", "runner_name": "", "steps": [], "labels": ["macos-26"]}'
ran='{"id": 0, "conclusion": "failure", "runner_name": "GitHub Actions 7", "steps": [{"status": "completed", "conclusion": "failure"}], "labels": ["ubuntu-latest"]}'
ok='{"id": 0, "conclusion": "success", "runner_name": "GitHub Actions 8", "steps": [{"status": "completed", "conclusion": "success"}], "labels": ["macos-26"]}'
job() { jq -c --argjson i "$1" --arg n "$2" '.id = $i | .name = $n' <<<"$3"; }
jobs_json() { printf '{"jobs": [%s]}' "$(
  IFS=,
  echo "$*"
)"; }
run41857275799=$(jobs_json "$(job 10 areas "$ok")" "$(job 11 "macos / ci/macos" "$never")" \
  "$(job 12 ios '{"conclusion": "skipped", "runner_name": null, "steps": [], "labels": []}')" \
  "$(job 13 "detection / ci/detection (1)" "$never")" "$(job 14 "detection / ci/detection (2)" "$never")" \
  "$(job 15 "detection / ci/detection" "$ran")" "$(job 16 "detection / detection shards" "$ok")")
eq "$(failure_kind 11 <<<"$run41857275799")" "infra macOS"
eq "$(failure_kind 15 <<<"$run41857275799")" "infra macOS"
# A shard that ran and failed: the summary's failure is a result.
eq "$(failure_kind 15 <<<"$(jobs_json "$(job 13 "detection / ci/detection (1)" "$ran")" \
  "$(job 14 "detection / ci/detection (2)" "$never")" "$(job 15 "detection / ci/detection" "$ran")")")" "result"
# A job cancelled after it started (#635: run 42154709474's ci/ios (tests),
# "GitHub Actions has encountered an internal error"): no result either;
# its annotations decide (a timeout is one). A summary over such a part,
# with or without one that never ran, the same; a part that failed outright,
# or a job that timed out (conclusion timed_out), is a result.
cut='{"id": 0, "conclusion": "cancelled", "runner_name": "GitHub Actions 1000010371", "steps": [{"status": "completed", "conclusion": "success"}, {"status": "completed", "conclusion": "cancelled"}], "labels": ["macos-26"]}'
eq "$(failure_kind 11 <<<"$(jobs_json "$(job 11 "macos / ci/macos" "$cut")")")" "cancelled macOS 11"
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$cut")" "$(job 32 "ios / ci/ios-release" "$ok")" "$(job 30 "ios / ci/ios" "$ran")")")" "cancelled macOS 31"
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$cut")" "$(job 32 "ios / ci/ios-release" "$never")" "$(job 30 "ios / ci/ios" "$ran")")")" "cancelled macOS 31"
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$cut")" "$(job 32 "ios / ci/ios-release" "$cut")" "$(job 30 "ios / ci/ios" "$ran")")")" "cancelled macOS 31 32"
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$cut")" "$(job 32 "ios / ci/ios-release" "$ran")" "$(job 30 "ios / ci/ios" "$ran")")")" "result"
# A summary cancelled itself: its parts are looked at too.
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$ok")" "$(job 30 "ios / ci/ios" "${cut/macos-26/ubuntu-latest}")")")" "cancelled Linux 30"
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$ran")" "$(job 30 "ios / ci/ios" "${cut/macos-26/ubuntu-latest}")")")" "result"
eq "$(failure_kind 11 <<<"$(jobs_json "$(job 11 "macos / ci/macos" "$(jq -c '.conclusion = "timed_out"' <<<"$cut")")")")" "result"
# A summary that timed out itself, over cancelled parts: a result (Codex
# review on #636).
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$cut")" "$(job 30 "ios / ci/ios" "$(jq -c '.conclusion = "timed_out"' <<<"$ran")")")")" "result"
# A missing summary's parts: the first real failure, else those cut.
eq "$(missing_parts ci/ios <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$cut")" "$(job 32 "ios / ci/ios-release" "$never")" "$(job 11 "macos / ci/macos" "$ran")")")" "cut 31"
eq "$(missing_parts ci/ios <<<"$(jobs_json "$(job 32 "ios / ci/ios-release" "$never")")")" "cut "
eq "$(missing_parts ci/ios <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$cut")" "$(job 32 "ios / ci/ios-release" "$(jq -c '.html_url = "https://r"' <<<"$ran")")")")" "result https://r"
printf 'The job running on runner GitHub Actions 3 has exceeded the maximum execution time of 40 minutes.\n' | timed_out || fail "a timeout not seen"
if printf 'GitHub Actions has encountered an internal error when running your job.\n' | timed_out; then fail "an internal error taken for a timeout"; fi
# Summaries over parts named after them (ci/macos's two jobs, ci/ios with
# ci/ios-release): infra only when every failing part never ran; another
# caller's jobs, and jobs not named after it, are not its parts.
eq "$(failure_kind 20 <<<"$(jobs_json "$(job 21 "macos / ci/macos (lint)" "$never")" \
  "$(job 22 "macos / ci/macos (tests)" "$ok")" "$(job 20 "macos / ci/macos" "$ran")")")" "infra macOS"
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 31 "ios / ci/ios (tests)" "$ran")" \
  "$(job 32 "ios / ci/ios-release" "$never")" "$(job 30 "ios / ci/ios" "$ran")")")" "result"
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 32 "ios / ci/ios-release" "$never")" "$(job 30 "ios / ci/ios" "$ran")")")" "infra macOS"
eq "$(failure_kind 30 <<<"$(jobs_json "$(job 11 "macos / ci/macos" "$never")" "$(job 30 "ios / ci/ios" "$ran")")")" "result"
eq "$(failure_kind 40 <<<"$(jobs_json "$(job 41 "lint (ubuntu)" "$never")" "$(job 40 "fast (ubuntu)" "$ran")")")" "result"
eq "$(failure_kind 40 <<<"$(jobs_json "$(job 41 "fast (unit)" "${never/macos-26/ubuntu-latest}")" "$(job 40 "fast (ubuntu)" "$ran")")")" "infra Linux"
# Not among the latest jobs: a re-run replaced it (Codex review on #619).
eq "$(failure_kind 99 <<<"$run41857275799")" "superseded"
echo "ok: a job that never ran, by runner and steps"

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
eq "$(printf '\n' | queue_gate https://run)" "success|Docs only: no job runs|https://run"
echo "ok: a queue commit's gate"

# --- one pass against fakes --------------------------------------------------

posted="$tmp/posted" dispatched="$tmp/dispatched"
: >"$posted"
: >"$dispatched"
now() { echo 1793620800; }
fetch_heads() { printf '1 false aaaa 0 false\n2 false bbbb 0 false\n3 true cccc 0 false\n4 false dddd 0 false\n5 false eeee 0 false\n'; }
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
fetch_file() { cat "$2"; }
# The commit's Queue run, still going; a job's annotations, none.
queue_run="in_progress 42 1 null https://github.com/lawkitt/mdoc/actions/runs/42"
fetch_queue_run() { echo "$queue_run"; }
fetch_annotations() { :; }

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

# No hosted Android job on GitHub (ci-android.yml a 404): the Android job fails
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
# The earlier group's statuses cannot be read: the PC may be busy there, so
# wait (Codex review on #588); main's the same.
for down in pppp mmmm; do
  fetch_status() {
    case "$1" in
      "$down") return 1 ;;
      qqqq) echo '{"statuses": [{"context": "ci/windows", "state": "success", "updated_at": "2026-11-02T11:50:00Z", "target_url": "https://pc/w"}]}' ;;
      *) echo '{"statuses": []}' ;;
    esac
  }
  : >"$posted"
  # The earlier queue commit is skipped: red. Main is read only for the PC's
  # last post, and gh_call's error list turns that pass red (gate_main).
  if queue_pass; then [ "$down" = mmmm ] || fail "a pass that could not read $down ended green"; fi
  eq "$(grep '^qqqq' "$posted")" "qqqq pending Waiting: ios, android; passed: macos (hosted), windows (PC), detection (hosted)"
done
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
if queue_pass; then fail "a pass that could not read a workflow ended green"; fi
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

# --- ADR 0100: errors, jobs that never ran, the area table, the follow-up ----

# An error on one PR: the others are decided, the skip is logged, the pass is red.
: >"$posted"
: >"$logf"
fetch_check_runs() {
  case "$1" in
    dddd) return 1 ;;
    bbbb) echo '{"check_runs": [{"id": 1, "name": "fast (ubuntu)", "status": "completed", "conclusion": "success", "html_url": "https://f"}]}' ;;
    *) echo '{"check_runs": []}' ;;
  esac
}
fetch_status() { echo '{"statuses": []}'; }
if heads_pass >/dev/null; then fail "a pass that skipped a PR ended green"; fi
grep -qxF "#4: skipped: could not read the check runs of dddd" "$logf" || fail "the skipped PR is not logged"
eq "$(cut -d' ' -f1 "$posted" | sort | tr '\n' ' ')" "aaaa bbbb eeee "
# The same on a queue commit: nothing posted for it, logged, red.
for broken in fetch_compare fetch_check_runs fetch_status fetch_commit_time fetch_pr fetch_dispatched; do
  saved_fn=$(declare -f "$broken")
  eval "$broken() { return 1; }"
  : >"$posted"
  : >"$logf"
  if queue_pass; then fail "a queue pass with a failing $broken ended green"; fi
  grep -q '^qqqq: skipped: could not read ' "$logf" || fail "$broken: the skipped commit is not logged"
  eq "$(cat "$posted")" ""
  eval "$saved_fn"
done
echo "ok: a PR head or queue commit skipped by an error is logged, and the pass is red"

# A hosted job that never ran (no runner, no step): re-run, and wait.
url901=https://github.com/lawkitt/mdoc/actions/runs/900/job/901
fetch_check_runs() {
  printf '{"check_runs": [
    {"id": 5, "name": "macos / ci/macos", "status": "completed", "conclusion": "success", "html_url": "https://mac", "completed_at": "2026-11-02T11:30:00Z"},
    {"id": 3, "name": "detection / ci/detection", "status": "completed", "conclusion": "success", "html_url": "https://d", "completed_at": "2026-11-02T11:21:00Z"},
    {"id": 901, "name": "ios / ci/ios", "status": "completed", "conclusion": "%s", "html_url": "%s", "completed_at": "2026-11-02T11:40:00Z"}]}\n' \
    "${ios_conclusion:-cancelled}" "$url901"
}
fetch_status() { echo '{"statuses": [{"context": "ci/windows", "state": "success", "updated_at": "2026-11-02T11:50:00Z"}, {"context": "ci/android", "state": "success", "updated_at": "2026-11-02T11:51:00Z"}]}'; }
ios901=$never
fetch_run_jobs() { jobs_json "$(job 900 "areas" "$ok")" "$(job 901 "ios / ci/ios" "$ios901")"; }
run_state="completed 1"
fetch_run() { echo "$run_state"; }
reran="$tmp/reran"
rerun_failed() { echo "$1" >>"$reran"; }
queue_once() {
  : >"$posted"
  : >"$reran"
  queue_pass
}
queue_once
eq "$(cat "$reran")" 900
eq "$(cat "$posted")" "qqqq pending Waiting: ios; passed: macos (hosted), windows (PC), android (PC), detection (hosted)"
# Its re-run still going: wait, no second re-run.
run_state="in_progress 2"
queue_once
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "qqqq pending Waiting: ios; passed: macos (hosted), windows (PC), android (PC), detection (hosted)"
# A second time: one re-run left.
run_state="completed 2"
queue_once
eq "$(cat "$reran")" 900
# A third time: it fails, worded as infrastructure, linking the job.
run_state="completed 3"
queue_once
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "qqqq failure Failed: ios (no macOS runner, 3 attempts); passed: macos (hosted), windows (PC), android (PC), detection (hosted)"
# A job that ran and failed fails at once, never re-run.
ios_conclusion=failure ios901=$ran run_state="completed 1"
queue_once
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "qqqq failure Failed: ios (hosted); passed: macos (hosted), windows (PC), android (PC), detection (hosted)"
# Beside a real failure, a job that never ran is not re-run: the commit fails anyway.
ios_conclusion=cancelled ios901=$never
fetch_status() { echo '{"statuses": [{"context": "ci/windows", "state": "failure", "updated_at": "2026-11-02T11:50:00Z", "target_url": "https://pc/w"}, {"context": "ci/android", "state": "success", "updated_at": "2026-11-02T11:51:00Z"}]}'; }
queue_once
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "qqqq failure Failed: windows (PC); passed: macos (hosted), android (PC), detection (hosted)"
echo "ok: a hosted job that never ran is re-run twice, then fails as infrastructure; a result fails at once"
# The check runs predate a re-run: job 901 is no longer among the latest
# jobs (902 replaced it). Wait, never a result (Codex review on #619).
fetch_status() { echo '{"statuses": [{"context": "ci/windows", "state": "success", "updated_at": "2026-11-02T11:50:00Z"}, {"context": "ci/android", "state": "success", "updated_at": "2026-11-02T11:51:00Z"}]}'; }
fetch_run_jobs() { jobs_json "$(job 902 "ios / ci/ios" '{"conclusion": null, "runner_name": "", "steps": []}')"; }
run_state="queued 2"
queue_once
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "qqqq pending Waiting: ios; passed: macos (hosted), windows (PC), android (PC), detection (hosted)"
# A hosted Windows fallback that never ran is decided with the other jobs:
# re-run while nothing failed; beside a real failure, not (Codex review on #619).
fetch_check_runs() {
  printf '{"check_runs": [
    {"id": 5, "name": "macos / ci/macos", "status": "completed", "conclusion": "%s", "html_url": "https://mac", "completed_at": "2026-11-02T11:30:00Z"},
    {"id": 6, "name": "ios / ci/ios", "status": "completed", "conclusion": "success", "html_url": "https://ios", "completed_at": "2026-11-02T11:30:00Z"},
    {"id": 3, "name": "detection / ci/detection", "status": "completed", "conclusion": "success", "html_url": "https://d", "completed_at": "2026-11-02T11:21:00Z"},
    {"id": 801, "name": "ci/windows (hosted)", "status": "completed", "conclusion": "cancelled", "html_url": "https://github.com/lawkitt/mdoc/actions/runs/800/job/801", "completed_at": "2026-11-02T11:40:00Z"}]}\n' "$mac"
}
fetch_run_jobs() { jobs_json "$(job 801 "ci/windows (hosted)" "${never/macos-26/windows-2025}")"; }
fetch_status() { echo '{"statuses": [{"context": "ci/android", "state": "success", "updated_at": "2026-11-02T11:51:00Z"}]}'; }
fetch_dispatched() { echo 1; }
run_state="completed 1" mac=success
queue_once
eq "$(cat "$reran")" 800
eq "$(cat "$posted")" "qqqq pending Waiting: windows; passed: macos (hosted), ios (hosted), android (PC), detection (hosted)"
run_state="completed 3"
queue_once
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "qqqq failure Failed: windows (no Windows runner, 3 attempts); passed: macos (hosted), ios (hosted), android (PC), detection (hosted)"
run_state="completed 1" mac=failure
queue_once
eq "$(cat "$reran")" ""
eq "$(cut -d' ' -f1-4 "$posted")" "qqqq failure Failed: macos"
fetch_dispatched() { echo 0; }
echo "ok: a re-run's replaced job waits; a hosted fallback that never ran is re-run with the commit's other jobs"

# A summary over shards that never ran (ci/detection, run 41857275799).
ios_conclusion=success
fetch_status() { echo '{"statuses": [{"context": "ci/windows", "state": "success", "updated_at": "2026-11-02T11:50:00Z"}, {"context": "ci/android", "state": "success", "updated_at": "2026-11-02T11:51:00Z"}]}'; }
fetch_check_runs() {
  echo '{"check_runs": [
    {"id": 5, "name": "macos / ci/macos", "status": "completed", "conclusion": "success", "html_url": "https://mac", "completed_at": "2026-11-02T11:30:00Z"},
    {"id": 6, "name": "ios / ci/ios", "status": "completed", "conclusion": "success", "html_url": "https://ios", "completed_at": "2026-11-02T11:30:00Z"},
    {"id": 15, "name": "detection / ci/detection", "status": "completed", "conclusion": "failure", "html_url": "https://github.com/lawkitt/mdoc/actions/runs/41857275799/job/15", "completed_at": "2026-11-02T11:21:00Z"}]}'
}
fetch_run_jobs() { echo "$run41857275799"; }
run_state="completed 1"
queue_once
eq "$(cat "$reran")" 41857275799
eq "$(cat "$posted")" "qqqq pending Waiting: detection; passed: macos (hosted), windows (PC), ios (hosted), android (PC)"
echo "ok: a detection summary over shards that never ran is re-run"

# A part cancelled after it started (#635: an internal error): re-run, then
# infrastructure; one that hit its own timeout-minutes fails at once.
fetch_check_runs() {
  printf '{"check_runs": [
    {"id": 5, "name": "macos / ci/macos", "status": "completed", "conclusion": "success", "html_url": "https://mac", "completed_at": "2026-11-02T11:30:00Z"},
    {"id": 3, "name": "detection / ci/detection", "status": "completed", "conclusion": "success", "html_url": "https://d", "completed_at": "2026-11-02T11:21:00Z"},
    {"id": 901, "name": "ios / ci/ios", "status": "completed", "conclusion": "failure", "html_url": "%s", "completed_at": "2026-11-02T11:40:00Z"}]}\n' "$url901"
}
fetch_run_jobs() { jobs_json "$(job 902 "ios / ci/ios (tests)" "$cut")" "$(job 903 "ios / ci/ios-release" "$ok")" "$(job 901 "ios / ci/ios" "$ran")"; }
notes_read="$tmp/notes-read"
fetch_annotations() {
  echo "$1" >>"$notes_read"
  printf '%s\n' "$ios_notes"
}
ios_notes="GitHub Actions has encountered an internal error when running your job."
run_state="completed 1"
: >"$notes_read"
queue_once
eq "$(cat "$reran")|$(cat "$notes_read")" "900|902"
eq "$(cat "$posted")" "qqqq pending Waiting: ios; passed: macos (hosted), windows (PC), android (PC), detection (hosted)"
run_state="completed 3"
queue_once
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "qqqq failure Failed: ios (cancelled on macOS, 3 attempts); passed: macos (hosted), windows (PC), android (PC), detection (hosted)"
ios_notes="The job running on runner GitHub Actions 1000010371 has exceeded the maximum execution time of 40 minutes."
run_state="completed 1"
queue_once
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "qqqq failure Failed: ios (hosted); passed: macos (hosted), windows (PC), android (PC), detection (hosted)"
# Annotations that cannot be read: the commit is skipped, the pass red.
fetch_annotations() { return 1; }
: >"$logf"
if queue_once; then fail "a pass that could not read annotations ended green"; fi
grep -qxF "qqqq: skipped: could not read the run behind ci/ios" "$logf" || fail "the skip is not logged"
fetch_annotations() { :; }
echo "ok: a job cancelled after it started is re-run, then fails as infrastructure; a timeout fails at once"

# A queue commit whose summary job was never created (#635: Queue run
# 42154709474, attempt 1, on #633's queue commit): ci/ios (tests) cancelled
# by an internal error, ci/ios-release passed, no ci/ios. Missing while the
# run goes on is pending; once it has completed, its failed jobs are re-run
# twice, then the job fails as "did not run".
saved_compare=$(declare -f fetch_compare)
fetch_compare() {
  # shellcheck disable=SC2028
  echo '{"files": [{"filename": "apps/ios/mdoc/FindSheet.swift"}, {"filename": "scripts/merge-gate.sh"}], "commits": [{"commit": {"message": "iOS Find (#633)"}}]}'
}
run635=https://github.com/lawkitt/mdoc/actions/runs/42154709474
checks635() { # extra check-run JSON
  printf '{"check_runs": [
    {"id": 127686035886, "name": "scripts / ci/scripts (bash 3.2)", "status": "completed", "conclusion": "success", "html_url": "%s/job/127686035886", "completed_at": "2026-11-06T03:11:03Z"},
    {"id": 127686036095, "name": "scripts / ci/scripts (linux)", "status": "completed", "conclusion": "success", "html_url": "%s/job/127686036095", "completed_at": "2026-11-06T03:10:41Z"},
    {"id": 127686787415, "name": "scripts / ci/scripts", "status": "completed", "conclusion": "%s", "html_url": "%s/job/127686787415", "completed_at": "2026-11-06T03:11:09Z"},
    {"id": 127686035978, "name": "ios / ci/ios-release", "status": "completed", "conclusion": "success", "html_url": "%s/job/127686035978", "completed_at": "2026-11-06T03:18:51Z"},
    {"id": 127686036018, "name": "ios / ci/ios (tests)", "status": "completed", "conclusion": "cancelled", "html_url": "%s/job/127686036018", "completed_at": "2026-11-06T03:19:26Z"}%s]}\n' \
    "$run635" "$run635" "${scripts635:-success}" "$run635" "$run635" "$run635" "${1:-}"
}
check_reads="$tmp/check-reads"
fetch_check_runs() {
  echo "$1" >>"$check_reads"
  checks635
}
queue635() {
  : >"$check_reads"
  queue_once
}
# Its jobs: the tests part as $tests635 (cancelled after it started).
jobs635() {
  jobs_json "$(job 127686035978 "ios / ci/ios-release" "$ok")" \
    "$(job 127686036018 "ios / ci/ios (tests)" "$(jq -c --arg u "$run635/job/127686036018" '.html_url = $u' <<<"$tests635")")" \
    "$(job 127686787415 "scripts / ci/scripts" "$ok")"
}
tests635=$cut
fetch_run_jobs() { jobs635; }
ios_notes="GitHub Actions has encountered an internal error when running your job."
fetch_annotations() {
  echo "$1" >>"$notes_read"
  printf '%s\n' "$ios_notes"
}
queue_run="in_progress 42154709474 1 null $run635"
queue635
eq "$(cat "$reran")|$(wc -l <"$check_reads" | tr -d ' ')" "|1"
eq "$(cat "$posted")" "qqqq pending Waiting: ios; passed: scripts (hosted)"
# Completed: the check runs are read again, then the run is re-run.
queue_run="completed 42154709474 1 failure $run635"
queue635
eq "$(cat "$reran")|$(wc -l <"$check_reads" | tr -d ' ')" "42154709474|2"
eq "$(cat "$posted")" "qqqq pending Waiting: ios; passed: scripts (hosted)"
queue_run="completed 42154709474 2 failure $run635"
queue635
eq "$(cat "$reran")" 42154709474
queue_run="completed 42154709474 3 failure $run635"
post() { echo "$1 $2 $3 $4" >>"$posted"; }
queue635
post() { echo "$1 $2 $3" >>"$posted"; }
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "qqqq failure Failed: ios (did not run, 3 attempts); passed: scripts (hosted) $run635"
# Its tests part hit its own timeout-minutes, or failed outright: a result,
# never re-run, linking that part (Codex review on #636).
ios_notes="The job running on runner GitHub Actions 1000010371 has exceeded the maximum execution time of 40 minutes."
queue_run="completed 42154709474 1 failure $run635"
post() { echo "$1 $2 $3 $4" >>"$posted"; }
: >"$notes_read"
queue635
eq "$(cat "$reran")|$(cat "$notes_read")|$(cat "$posted")" "|127686036018|qqqq failure Failed: ios (hosted); passed: scripts (hosted) $run635/job/127686036018"
ios_notes="GitHub Actions has encountered an internal error when running your job."
tests635=$ran
queue635
eq "$(cat "$reran")|$(cat "$posted")" "|qqqq failure Failed: ios (hosted); passed: scripts (hosted) $run635/job/127686036018"
post() { echo "$1 $2 $3" >>"$posted"; }
# Its jobs cannot be read: the commit is skipped.
fetch_run_jobs() { return 1; }
: >"$logf"
if queue635; then fail "a pass that could not read the Queue run's jobs ended green"; fi
grep -qxF "qqqq: skipped: could not read the run behind ci/ios" "$logf" || fail "the skip is not logged"
tests635=$cut
fetch_run_jobs() { jobs635; }
# The summary ended between the two reads: its result counts.
fetch_check_runs() {
  echo "$1" >>"$check_reads"
  if [ "$(wc -l <"$check_reads")" -lt 2 ]; then checks635; else
    checks635 ', {"id": 127686990000, "name": "ios / ci/ios", "status": "completed", "conclusion": "success", "html_url": "https://ios", "completed_at": "2026-11-06T03:20:00Z"}'
  fi
}
queue_run="completed 42154709474 1 success $run635"
queue635
eq "$(cat "$reran")|$(cat "$posted")" "|qqqq success Passed: ios (hosted), scripts (hosted)"
# A run that succeeded without it has nothing to re-run: it fails at once.
fetch_check_runs() { checks635; }
queue635
eq "$(cat "$reran")|$(cat "$posted")" "|qqqq failure Failed: ios (did not run); passed: scripts (hosted)"
# Beside a real failure (ci/scripts failed in the same run) it is not re-run.
scripts635=failure
fetch_run_jobs() { jobs_json "$(job 127686035886 "scripts / ci/scripts (bash 3.2)" "$ran")" "$(job 127686787415 "scripts / ci/scripts" "$ran")"; }
queue_run="completed 42154709474 1 failure $run635"
queue635
eq "$(cat "$reran")|$(cat "$posted")" "|qqqq failure Failed: scripts (hosted)"
# No Queue run yet: pending; one that cannot be read skips the commit.
scripts635=success queue_run=''
queue635
eq "$(cat "$reran")|$(cat "$posted")" "|qqqq pending Waiting: ios; passed: scripts (hosted)"
fetch_queue_run() { return 1; }
: >"$logf"
if queue635; then fail "a pass that could not read the Queue run ended green"; fi
grep -qxF "qqqq: skipped: could not read its Queue run" "$logf" || fail "the skip is not logged"
fetch_queue_run() { echo "$queue_run"; }
queue_run="in_progress 42 1 null https://github.com/lawkitt/mdoc/actions/runs/42"
eval "$saved_compare"
eq "$(missing_verdict "")|$(missing_verdict "queued 7 1 null https://r")" "pending - 0|pending - 0"
echo "ok: a hosted check missing once its Queue run completed is re-run, then fails as did not run"

# A fast tier that never ran on a PR head: re-run; out of re-runs, it fails
# as infrastructure.
fetch_heads() { echo "2 false bbbb 0 false"; }
fetch_pr_files() { echo src/main.rs; }
fetch_check_runs() { echo '{"check_runs": [{"id": 41, "name": "fast (ubuntu)", "status": "completed", "conclusion": "cancelled", "html_url": "https://github.com/lawkitt/mdoc/actions/runs/700/job/41"}]}'; }
fetch_run_jobs() { jobs_json "$(job 41 "fast (ubuntu)" "${never/macos-26/ubuntu-latest}")"; }
fetch_status() { echo '{"statuses": []}'; }
: >"$posted"
: >"$reran"
heads_pass
eq "$(cat "$reran")" 700
eq "$(cat "$posted")" "bbbb pending Waiting for the fast tier: fast (ubuntu)"
run_state="completed 3"
: >"$posted"
: >"$reran"
heads_pass
eq "$(cat "$reran")" ""
eq "$(cat "$posted")" "bbbb failure Fast tier did not run (no Linux runner, 3 attempts): fast (ubuntu)"
echo "ok: a fast tier that never ran is re-run, then fails as infrastructure"

# Detection on PR heads (ADR 0100). #575 changes a fixture's snapshot, #2 the
# app: its head has its fast tier alone, #575's head the checks of
# ffff_checks.
fetch_heads() { printf '2 false bbbb 0 false\n575 false ffff 0 false\n'; }
fetch_pr_files() {
  case "$1" in
    575) echo tests/fixtures/recognition/en/letter/x.model.json ;;
    *) echo src/main.rs ;;
  esac
}
fast_run='{"id": 41, "name": "fast (ubuntu)", "status": "completed", "conclusion": "success", "html_url": "https://github.com/lawkitt/mdoc/actions/runs/700/job/41", "completed_at": "2026-11-02T11:40:00Z"}'
det_url=https://github.com/lawkitt/mdoc/actions/runs/700/job/51
det_runs() { # status conclusion
  printf '{"check_runs": [%s,
    {"id": 51, "name": "detection-head / ci/detection", "status": "%s", "conclusion": %s, "html_url": "%s", "completed_at": "2026-11-02T11:45:00Z"},
    {"id": 52, "name": "detection-head / ci/detection (1)", "status": "%s", "conclusion": %s, "html_url": "https://d1"}]}\n' \
    "$fast_run" "$1" "$2" "$det_url" "$1" "$2"
}
heads_once() {
  : >"$posted"
  : >"$reran"
  recheck=''
  heads_pass
}
fetch_check_runs() {
  if [ "$1" = bbbb ]; then printf '{"check_runs": [%s]}\n' "$fast_run"; else ffff_checks; fi
}
# Its head detection still running: pending, a follow-up pass; the app PR
# is green on its fast tier alone.
ffff_checks() { det_runs in_progress null; }
heads_once
eq "$(sort "$posted")" "bbbb success Fast tier passed (unit, scripts); the full suite runs in the merge queue
ffff pending Fast tier passed; waiting for the detection gate: detection-head / ci/detection"
eq "$recheck" "#575 waits for its head checks"
# Not started yet: no check runs of it at all while the CI run goes on.
ffff_checks() { printf '{"check_runs": [%s]}\n' "$fast_run"; }
run_event="in_progress pull_request"
fetch_run_event() { echo "$run_event"; }
heads_once
eq "$(grep ffff "$posted")" "ffff pending Fast tier passed; waiting for the detection gate: detection-head / ci/detection"
# CI's own run starts the next pass, so no follow-up pass for a job that
# never started (it would repeat every 3 minutes).
eq "$recheck" ""
# A PR's CI run that finished without it (a ci.yml from before the head
# runs: #579, #599 and #600 on 2026-11-06) is not waited for: the queue runs
# it. A finished manual run of ci.yml on the same commit is no such run
# (Codex review on #625).
run_event="completed pull_request"
heads_once
eq "$(grep ffff "$posted")" "ffff success Fast tier passed (unit, scripts); the full suite runs in the merge queue"
run_event="completed workflow_dispatch"
heads_once
eq "$(grep ffff "$posted")" "ffff pending Fast tier passed; waiting for the detection gate: detection-head / ci/detection"
eq "$(run_of "success https://github.com/lawkitt/mdoc/actions/runs/700/job/41 1")$(run_of "pending - 0")" 700
# Passed.
ffff_checks() { det_runs completed '"success"'; }
heads_once
eq "$(grep ffff "$posted")" "ffff success Fast tier and detection gate passed; the full suite runs in the merge queue"
eq "$recheck" ""
# A stale snapshot: red on the head, linking the run with the snapshots.
post() { echo "$1 $2 $3 $4" >>"$posted"; }
ffff_checks() { det_runs completed '"failure"'; }
ran52=$(job 52 "detection-head / ci/detection (1)" "$ran")
fetch_run_jobs() { jobs_json "$(job 41 "fast (ubuntu)" "$ok")" "$ran52" "$(job 51 "detection-head / ci/detection" "$ran")"; }
heads_once
eq "$(grep ffff "$posted")" "ffff failure Detection gate failed: detection-head / ci/detection; regenerated snapshots: just snapshots-fetch 575 $det_url"
eq "$(cat "$reran")" ""
post() { echo "$1 $2 $3" >>"$posted"; }
# Shards that never ran: re-run, pending; out of re-runs, infrastructure.
never52=$(job 52 "detection-head / ci/detection (1)" "$never")
ffff_checks() { det_runs completed '"cancelled"'; }
fetch_run_jobs() { jobs_json "$(job 41 "fast (ubuntu)" "$ok")" "$never52" "$(job 51 "detection-head / ci/detection" "$ran")"; }
run_state="completed 1"
heads_once
eq "$(cat "$reran")" 700
eq "$(grep ffff "$posted")" "ffff pending Fast tier passed; waiting for the detection gate: detection-head / ci/detection"
run_state="completed 3"
heads_once
eq "$(cat "$reran")" ""
eq "$(grep ffff "$posted")" "ffff failure Detection gate did not run (no macOS runner, 3 attempts): detection-head / ci/detection"
# With the fast tier never run too: one re-run of their one run.
fetch_heads() { echo "575 false ffff 0 false"; }
ffff_checks() {
  printf '{"check_runs": [{"id": 41, "name": "fast (ubuntu)", "status": "completed", "conclusion": "cancelled", "html_url": "https://github.com/lawkitt/mdoc/actions/runs/700/job/41"},
    {"id": 51, "name": "detection-head / ci/detection", "status": "completed", "conclusion": "failure", "html_url": "%s"}]}\n' "$det_url"
}
fetch_run_jobs() { jobs_json "$(job 41 "fast (ubuntu)" "${never/macos-26/ubuntu-latest}")" "$never52" "$(job 51 "detection-head / ci/detection" "$ran")"; }
run_state="completed 1"
heads_once
eq "$(cat "$reran")" 700
eq "$(grep ffff "$posted")" "ffff pending Waiting for the fast tier and the detection gate: fast (ubuntu), detection-head / ci/detection"
# Skipped on the head (ci.yml's copy of the table said no): the fast tier
# alone, as for the app PR; the queue runs the gate.
fetch_heads() { printf '2 false bbbb 0 false\n575 false ffff 0 false\n'; }
ffff_checks() {
  printf '{"check_runs": [%s, {"id": 50, "name": "detection-head", "status": "completed", "conclusion": "skipped", "html_url": "https://github.com/lawkitt/mdoc/actions/runs/800/job/50"}]}\n' "$fast_run"
}
# The skip in a PR's run, while another run on the commit goes on.
run_event="in_progress pull_request"
heads_once
eq "$(sort "$posted")" "bbbb success Fast tier passed (unit, scripts); the full suite runs in the merge queue
ffff success Fast tier passed (unit, scripts); the full suite runs in the merge queue"
# A manual run of ci.yml skips it too, but waives nothing (Codex review on #625).
run_event="in_progress workflow_dispatch"
heads_once
eq "$(grep ffff "$posted")" "ffff pending Fast tier passed; waiting for the detection gate: detection-head / ci/detection"
# A head run that exists counts on any PR (Codex review on #625): main's
# table may not know a recognition line the PR adds, and a PR of 3000 files
# or more is not routed here.
fetch_check_runs() { det_runs completed '"failure"'; }
fetch_run_jobs() { jobs_json "$(job 41 "fast (ubuntu)" "$ok")" "$ran52" "$(job 51 "detection-head / ci/detection" "$ran")"; }
fetch_pr_files() {
  case "$1" in
    575) seq 3000 | sed 's|^|tests/fixtures/recognition/f|' ;;
    *) echo src/main.rs ;;
  esac
}
heads_once
eq "$(sort "$posted")" "bbbb failure Detection gate failed: detection-head / ci/detection; regenerated snapshots: just snapshots-fetch 2
ffff failure Detection gate failed: detection-head / ci/detection; regenerated snapshots: just snapshots-fetch 575"
echo "ok: heads: a recognition-area PR waits for its head detection (pending, passed, stale, never ran, skipped); a head run counts on any PR"

# The queue commit's own area table: one that makes everything docs needs
# no job; when GitHub cannot give it, main's.
fetch_check_runs() { echo '{"check_runs": []}'; }
fetch_file() { printf 'none  **\n'; }
: >"$posted"
queue_pass
eq "$(cat "$posted")" "qqqq success Docs only: no job runs"
fetch_file() {
  echo "gh: Not Found (HTTP 404)" >&2
  return 1
}
: >"$posted"
: >"$logf"
queue_pass 2>/dev/null
# Main's table: Cargo.lock (renamed away) is core, every job.
eq "$(cut -d' ' -f1-3 "$posted")" "qqqq failure Failed:"
grep -qxF "qqqq: could not read its scripts/ci-areas.txt; using main's" "$logf" || fail "no log of main's table"
fetch_file() { cat "$2"; }
echo "ok: the queue commit's own area table, else main's"

# The CI machinery alone (ADR 0100): only ci/scripts, the summary of the
# suites on Linux and under bash 3.2; no desktop, PC or detection job, and
# nothing dispatched for the silent PC.
fetch_compare() {
  # shellcheck disable=SC2028
  echo '{"files": [{"filename": "scripts/merge-gate.sh"}, {"filename": ".github/workflows/ci.yml"}], "commits": [{"commit": {"message": "Gate (#514)"}}]}'
}
fetch_status() { echo '{"statuses": []}'; }
scripts_runs() { # ci/scripts' conclusion
  printf '{"check_runs": [
    {"id": 951, "name": "scripts / ci/scripts (linux)", "status": "completed", "conclusion": "success", "html_url": "https://l", "completed_at": "2026-11-02T11:39:00Z"},
    {"id": 952, "name": "scripts / ci/scripts", "status": "completed", "conclusion": "%s", "html_url": "https://github.com/lawkitt/mdoc/actions/runs/950/job/952", "completed_at": "2026-11-02T11:40:00Z"}]}\n' "$1"
}
fetch_check_runs() { echo '{"check_runs": []}'; }
: >"$posted"
: >"$dispatched"
queue_pass
eq "$(cat "$posted")|$(cat "$dispatched")" "qqqq pending Waiting: scripts|"
fetch_check_runs() { scripts_runs success; }
: >"$posted"
queue_pass
eq "$(cat "$posted")" "qqqq success Passed: scripts (hosted)"
# A suite that ran and failed fails it at once.
fetch_check_runs() { scripts_runs failure; }
fetch_run_jobs() { jobs_json "$(job 950 "scripts / ci/scripts (bash 3.2)" "$ran")" "$(job 952 "scripts / ci/scripts" "$ran")"; }
run_state="completed 1"
queue_once
eq "$(cat "$reran")|$(cat "$posted")" "|qqqq failure Failed: scripts (hosted)"
# Its bash 3.2 part got no macOS runner: re-run, and wait.
fetch_run_jobs() { jobs_json "$(job 950 "scripts / ci/scripts (bash 3.2)" "$never")" "$(job 952 "scripts / ci/scripts" "$ran")"; }
queue_once
eq "$(cat "$reran")|$(cat "$posted")" "950|qqqq pending Waiting: scripts"
# With a desktop change, ci/macos runs the suites: no ci/scripts.
fetch_compare() {
  # shellcheck disable=SC2028
  echo '{"files": [{"filename": "scripts/merge-gate.sh"}, {"filename": "src/main.rs"}], "commits": [{"commit": {"message": "Gate (#514)"}}]}'
}
fetch_check_runs() { echo '{"check_runs": []}'; }
fetch_workflow_state() { echo active; }
: >"$posted"
: >"$dispatched"
queue_pass
eq "$(cat "$posted")|$(cut -d' ' -f1 "$dispatched")" "qqqq pending Waiting: macos, windows, detection|ci-desktop.yml"
echo "ok: a CI-only queue commit needs only ci/scripts"

# Missed enqueue (#613): green, auto-merge on, not in the queue.
t=1793620800
fetch_heads() { echo "2 false bbbb $m_since $m_queued"; }
fetch_check_runs() {
  echo '{"check_runs": [
    {"id": 1, "name": "fast (ubuntu)", "status": "completed", "conclusion": "success", "html_url": "https://f", "completed_at": "2026-11-02T11:40:00Z"},
    {"id": 2, "name": "lint (ubuntu)", "status": "completed", "conclusion": "success", "html_url": "https://l", "completed_at": "2026-11-02T11:41:00Z"}]}'
}
fetch_status() { printf '{"statuses": [{"context": "merge-gate", "state": "success", "updated_at": "%s", "description": "Fast tier passed (unit, scripts); the full suite runs in the merge queue"}]}\n' "$m_gate"; }
for case in "$((t - 900)) false 2026-11-02T11:50:00Z warn" "$((t - 900)) false 2026-11-02T11:57:00Z wait" \
  "$((t - 900)) true 2026-11-02T11:50:00Z none" "0 false 2026-11-02T11:50:00Z none"; do
  read -r m_since m_queued m_gate want <<<"$case"
  : >"$posted"
  : >"$logf"
  recheck=''
  heads_pass >"$tmp/out"
  eq "$(cat "$posted")" ""
  case "$want" in
    warn)
      eq "$(cat "$tmp/out")" "::warning title=Missed enqueue::#2 is green with auto-merge on but not in the merge queue after 10 min. Turn auto-merge off and on: gh pr merge 2 --disable-auto, then gh pr merge 2 --auto."
      eq "$recheck" ""
      ;;
    wait) eq "$(cat "$tmp/out")|$recheck" "|#2 waits for the merge queue" ;;
    none) eq "$(cat "$tmp/out")|$recheck" "|" ;;
  esac
done
echo "ok: a green PR with auto-merge on, out of the queue for 5 minutes, gets a warning"

# The follow-up pass: dispatched after an error or with a queue commit
# pending, not when all is settled.
rechecks="$tmp/rechecks"
dispatch_recheck() { echo recheck >>"$rechecks"; }
fetch_active_runs() { :; }
fetch_heads() { echo "1 false aaaa 0 false"; }
fetch_pr_files() { echo docs/x.md; }
fetch_status() { echo '{"statuses": []}'; }
fetch_check_runs() { echo '{"check_runs": []}'; }
fetch_compare() {
  # shellcheck disable=SC2028
  echo '{"files": [{"filename": "docs/x.md"}], "commits": [{"commit": {"message": "Docs (#514)"}}]}'
}
main_once() {
  : >"$rechecks"
  : >"$logf"
  rc=0
  gate_main >"$tmp/out" || rc=$?
}
main_once
eq "$rc|$(cat "$rechecks")" "0|"
# A queue commit waits for a job.
fetch_compare() {
  # shellcheck disable=SC2028
  echo '{"files": [{"filename": "apps/ios/App.swift"}], "commits": [{"commit": {"message": "iOS (#514)"}}]}'
}
main_once
eq "$rc|$(cat "$rechecks")" "0|recheck"
grep -qxF "follow-up pass in 3 minutes: qqqq is pending" "$logf" || fail "the follow-up pass is not logged"
# A GitHub call that keeps failing: red, an error annotation, a follow-up.
fetch_compare() {
  # shellcheck disable=SC2028
  echo '{"files": [{"filename": "docs/x.md"}], "commits": [{"commit": {"message": "Docs (#514)"}}]}'
}
gh() {
  echo "gh: Bad Gateway (HTTP 502)" >&2
  return 1
}
fetch_pr() { gh_call text api "repos/$repo/pulls/$1" --jq .user.type; }
main_once
eq "$rc|$(cat "$rechecks")" "1|recheck"
eq "$(cat "$tmp/out")" "::error title=GitHub calls failed::GET repos/$repo/pulls/514"
grep -qxF "qqqq: skipped: could not read PR #514" "$logf" || fail "the skipped commit is not logged"
# The follow-up cannot be dispatched: red too.
fetch_pr() { echo "User false"; }
fetch_compare() {
  # shellcheck disable=SC2028
  echo '{"files": [{"filename": "apps/ios/App.swift"}], "commits": [{"commit": {"message": "iOS (#514)"}}]}'
}
dispatch_recheck() { return 1; }
main_once
eq "$rc" 1
echo "ok: a pass with an error or a pending queue commit dispatches merge-gate-recheck.yml"

# Out of time (slow GitHub errors, Codex review on #619): the pass stops
# reading, ends red, and still dispatches the follow-up pass.
dispatch_recheck() { echo recheck >>"$rechecks"; }
: >"$posted"
saved_budget=$budget
budget=0
main_once
budget=$saved_budget
eq "$rc|$(cat "$rechecks")|$(cat "$posted")" "1|recheck|"
grep -qxF "out of time (0s): the follow-up pass reads the rest" "$logf" || fail "no out-of-time log"
eq "$(grep -c "out of time" "$logf")" 1
# A call past the deadline fails at once, without asking GitHub.
gh() {
  echo "$*" >>"$calls"
  echo '{}'
}
reset
deadline=1
if gh_call json api "repos/$repo/commits/abc/status" >/dev/null; then fail "a call past the deadline ran"; fi
deadline=0
eq "$(ncalls)" 0
echo "ok: a pass out of time stops reading and dispatches the follow-up pass"

gh() {
  echo "gh $*" >>"$tmp/unexpected-gh"
  return 1
}
[ ! -s "$tmp/unexpected-gh" ] || fail "real GitHub calls: $(cat "$tmp/unexpected-gh")"

echo "merge gate: all checks passed"
