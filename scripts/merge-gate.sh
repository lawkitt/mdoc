#!/usr/bin/env bash
# Usage: scripts/merge-gate.sh
#
# The `merge-gate` status (ADR 0095): the one required check beside `lint
# (ubuntu)`, posted on two kinds of commit.
#
#   A PR head: green when the fast tier (`fast (ubuntu)`) passed, and says
#   that the full suite runs in the merge queue; "Docs only" when the PR
#   touches no area (scripts/ci-areas.txt). A recognition-area PR (ADR 0100:
#   a file on the table's recognition lines, head_detection in lib.sh) also
#   waits for the detection gate on its head, `detection-head / ci/detection`
#   (ci.yml); a failure names `just snapshots-fetch <PR>`. That job skipped
#   by the PR's CI (its own copy of the table said no), or a finished PR CI
#   run without it (one from before this rule), counts as not needed: the
#   queue runs the gate anyway. A head run that exists counts on any PR, so
#   the PR's table and full diff decide when they say more than main's.
#   A queue commit (a branch under gh-readonly-queue/main/, or ci-shadow/main/
#   for the shadow trial): green only when every job its areas need passed,
#   each named with where it ran. Its areas come from its own
#   scripts/ci-areas.txt, as queue.yml and the PC read them (main's when
#   GitHub cannot give it). Hosted jobs (queue.yml) are check runs
#   named ci/macos, ci/ios, ci/detection (the detection gate's summary of
#   its shards) and ci/scripts (the CI machinery's script suites, ADR 0100);
#   a check counts only when it concluded success, or skipped
#   as not affected; anything else fails. The PC posts commit
#   statuses ci/windows and ci/android, and refreshes a pending one at least
#   every 5 minutes while it runs (its heartbeat).
#
# A hosted job that never ran (ADR 0100) is no result: cancelled with no
# runner and no step started, cancelled after it started (an internal error,
# a lost runner: GitHub reports a test failure as `failure`) unless it hit
# its own timeout-minutes, or a summary job whose failing parts (its
# caller's jobs named after it, `ci/detection (2)` for `ci/detection`) are
# all such jobs. Nor is a queue commit's hosted check that is missing once
# the commit's Queue run has completed (its summary job never created,
# #635). The gate re-runs the run's failed jobs, at most twice a run (by its
# run_attempt), and waits; after that it fails, worded as infrastructure
# ("no macOS runner, 3 attempts", "did not run, 3 attempts"). A job that ran
# and failed, or timed out, fails at once.
#
# Every GitHub call goes through gh_call: a time limit per try, 3 tries with
# backoff, and the endpoint named when it fails. A PR head or queue commit
# skipped because of an error is logged, and the run ends red. A pass that
# hit an error or left a queue commit (or a running fast tier) pending
# dispatches merge-gate-recheck.yml, which runs this again 3 minutes later.
# A green PR with auto-merge on that is still not in the queue after 5
# minutes gets a warning (#613).
#
# Fallback: a queue commit's Windows or Android job runs hosted (ci-desktop.yml
# with os=windows, ci-android.yml) when the PC has posted nothing 10 minutes
# after the commit appeared, when its pending status is 10 minutes old (a
# stale heartbeat), or at once when the group holds a fork or bot PR, which
# never runs on the PC. Whichever result finished first counts. A renamed
# file counts in the areas of both its old and its new path.
#
# With no hosted workflow to fall back to (GitHub has none of that name, or
# it is disabled), the job fails at once and says why, rather than
# leaving the group to the queue's 2-hour timeout: for third-party code, a
# PC runner error or a stale heartbeat; for a silent PC, only when the PC
# has posted that job on no queue commit and not on main for 10 minutes
# (a PC busy with an earlier group is still there).
#
# .github/workflows/merge-gate.yml runs main's copy of this script every 5
# minutes, after statuses and CI runs and when a recheck dispatches it, with
# statuses: write and actions: write; it never checks out PR code. Bash 3.2
# compatible (the suite runs under macOS /bin/bash).
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

repo=${MERGE_GATE_REPO:-${GITHUB_REPOSITORY:-lawkitt/mdoc}}
default_branch=${MERGE_GATE_BRANCH:-main}
context=merge-gate
prefixes=${MERGE_GATE_PREFIXES:-gh-readonly-queue/main/ ci-shadow/main/}
silent_min=${MERGE_GATE_SILENT_MIN:-10}
# A green PR with auto-merge on is late for the queue after this.
enqueue_min=${MERGE_GATE_ENQUEUE_MIN:-5}
# Re-runs of a run whose jobs never ran: attempts 1 and 2 are re-run.
reruns=${MERGE_GATE_RERUNS:-2}
# gh_call: tries per call, the first backoff in seconds (doubled each time),
# and each try's limit.
tries=${MERGE_GATE_TRIES:-3}
retry_delay=${MERGE_GATE_RETRY_DELAY:-2}
call_timeout=${MERGE_GATE_CALL_TIMEOUT:-30}
# A pass stops reading after this many seconds (merge-gate.yml allows 10
# minutes), so slow GitHub errors still leave time to dispatch the
# follow-up pass. gate_main sets the deadline; 0 means none.
budget=${MERGE_GATE_BUDGET:-360}
deadline=0
run_url=${GITHUB_SERVER_URL:-https://github.com}/$repo/actions/runs/${GITHUB_RUN_ID:-0}

# Scratch files: gh_call's stderr, and the calls that failed (subshells
# write there, so the pass sees errors from inside $(...)).
gate_tmp=${MERGE_GATE_TMP:-}
if [ -z "$gate_tmp" ]; then
  gate_tmp=$(mktemp -d)
  [ -n "${MERGE_GATE_SOURCED:-}" ] || trap 'rm -rf "$gate_tmp"' EXIT
fi
mkdir -p "$gate_tmp"
: >"$gate_tmp/errors"
timeout_cmd=$(command -v timeout || command -v gtimeout || true)

# The log goes to fd 3, the script's stderr, so `2>/dev/null` on a call
# hides gh's message but never the log.
exec 3>&2
log() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >&3; }

# jq for Windows ends lines with CRLF (the suite may run on the PC).
jq() { command jq "$@" | tr -d '\r'; }

# --- GitHub (replaced by fakes in scripts/test-merge-gate.sh) ----------------

# What a gh command line calls, for messages: `GET repos/...`, `POST graphql`,
# `workflow run x.yml`.
gh_what() { # gh-args...
  local method='' path='' prev='' a
  if [ "$1" != api ]; then
    echo "$1 $2 $3"
    return 0
  fi
  shift
  for a in "$@"; do
    [ "$prev" != -X ] || method=$a
    case "$a" in repos/* | graphql) [ -n "$path" ] || path=$a ;; esac
    prev=$a
  done
  if [ -z "$method" ]; then
    method=GET
    [ "$path" != graphql ] || method=POST
  fi
  echo "$method $path"
}

# Whether this pass is past its deadline. Says so once a pass.
out_of_time() {
  if [ "$deadline" -eq 0 ] || [ "$(date -u +%s)" -lt "$deadline" ]; then return 1; fi
  if [ ! -e "$gate_tmp/late" ]; then
    : >"$gate_tmp/late"
    log "out of time (${budget}s): the follow-up pass reads the rest"
  fi
}

gh_try() {
  if [ -n "$timeout_cmd" ] && [ "$(type -t gh)" = file ]; then
    "$timeout_cmd" "$call_timeout" gh "$@"
  else
    gh "$@"
  fi
}

# Runs `gh <args>` and prints its output: each try time-limited, $tries tries
# with backoff. With `json` the output must parse (a truncated body is
# retried); `text` takes it as it is (a call with --jq: gh parses it). A
# definitive answer (HTTP 404, 409 or 422) is not retried: it fails with
# gh's message on stderr, for the caller to read. Any other failure, after
# the last try, is logged with its endpoint and recorded: the pass ends red.
# Past the pass's deadline a call fails at once, and a failing one is not
# retried.
gh_call() { # json|text gh-args...
  local want=$1 try=1 delay=$retry_delay out rc err msg what
  shift
  if out_of_time; then
    echo "out of time: $(gh_what "$@")" >>"$gate_tmp/errors"
    return 1
  fi
  err=$(mktemp "$gate_tmp/err.XXXXXX")
  while :; do
    rc=0
    out=$(gh_try "$@" 2>"$err") || rc=$?
    if [ "$rc" -eq 0 ] && [ "$want" = json ] && { [ -z "$out" ] || ! command jq empty <<<"$out" 2>/dev/null; }; then
      rc=1
      echo "the response is not complete JSON" >"$err"
    fi
    if [ "$rc" -eq 0 ]; then
      rm -f "$err"
      printf '%s\n' "$out"
      return 0
    fi
    if grep -qE 'HTTP (404|409|422)' "$err"; then
      tr -d '\r' <"$err" >&2
      rm -f "$err"
      return "$rc"
    fi
    msg=$(tr -d '\r' <"$err" | awk 'NF { last = $0 } END { print last }')
    [ "$rc" -ne 124 ] || msg="no answer in ${call_timeout}s"
    if [ "$try" -ge "$tries" ] || out_of_time; then break; fi
    sleep "$delay"
    delay=$((delay * 2))
    try=$((try + 1))
  done
  rm -f "$err"
  what=$(gh_what "$@")
  log "GitHub call failed ($try tries): $what: ${msg:-exit $rc}"
  echo "$what" >>"$gate_tmp/errors"
  return 1
}

# shellcheck disable=SC2016 # GraphQL variables, not shell ones
heads_query='query($owner: String!, $name: String!, $endCursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 50, after: $endCursor) {
      pageInfo { hasNextPage endCursor }
      nodes { number isDraft headRefOid isInMergeQueue autoMergeRequest { enabledAt } }
    }
  }
}'

# Every open PR, all pages: `<number> <draft> <head> <auto-merge-since> <in-queue>`
# lines, the time in epoch seconds (0 when auto-merge is off).
fetch_heads() {
  gh_call json api graphql --paginate -f owner="${repo%/*}" -f name="${repo#*/}" -f query="$heads_query" |
    jq -r '.data.repository.pullRequests.nodes[]
      | "\(.number) \(.isDraft) \(.headRefOid) \(.autoMergeRequest.enabledAt // null | if . then fromdateiso8601 else 0 end) \(.isInMergeQueue)"'
}

# A PR's changed paths, a rename's old path too, one per line (GitHub lists
# at most 3000 files).
fetch_pr_files() {
  gh_call text api --paginate "repos/$repo/pulls/$1/files?per_page=100" --jq '.[] | .filename, (.previous_filename // empty)'
}

# The queue commits: `<branch> <sha>` lines.
fetch_queue() {
  local p
  for p in $prefixes; do
    gh_call text api "repos/$repo/git/matching-refs/heads/$p" \
      --jq '.[] | "\(.ref | sub("^refs/heads/"; "")) \(.object.sha)"' || return 1
  done
}

fetch_main() { gh_call text api "repos/$repo/commits/$default_branch" --jq .sha; }

# Runs not yet finished: `<id> <branch>` lines, queued and in progress.
fetch_active_runs() {
  local s
  for s in queued in_progress; do
    gh_call text api "repos/$repo/actions/runs?status=$s&per_page=100" --jq '.workflow_runs[] | "\(.id) \(.head_branch)"' || return 1
  done
}

# A run that finished meanwhile answers HTTP 409: nothing to cancel, and
# nothing to say.
cancel_run() { gh_call text api -X POST "repos/$repo/actions/runs/$1/cancel" >/dev/null 2>&1; }

# What a queue commit brings onto main: changed files and commit subjects.
fetch_compare() { gh_call json api "repos/$repo/compare/$1...$2"; }

# A file at a commit, raw; fails with "HTTP 404" when the commit has none.
fetch_file() { # sha path
  gh_call text api -H 'Accept: application/vnd.github.raw' "repos/$repo/contents/$2?ref=$1"
}

# When a commit was made, in epoch seconds.
fetch_commit_time() { gh_call text api "repos/$repo/commits/$1" --jq '.commit.committer.date | fromdateiso8601'; }

# Every check run on a commit, all pages, as one {"check_runs": [...]}.
fetch_check_runs() {
  gh_call text api --paginate "repos/$repo/commits/$1/check-runs?per_page=100" --jq '.check_runs[]' | jq -s '{check_runs: .}'
}

fetch_status() { gh_call json api "repos/$repo/commits/$1/status?per_page=100"; }

# `<author type> <cross-repository>` of PR $1.
fetch_pr() { gh_call text api "repos/$repo/pulls/$1" --jq '"\(.user.type) \(.head.repo.full_name != .base.repo.full_name)"'; }

# A workflow's state on GitHub (active, disabled_manually, ...); fails with
# "HTTP 404" in its error when the repository has no such workflow.
fetch_workflow_state() { gh_call text api "repos/$repo/actions/workflows/$1" --jq .state; }

# How many manual runs of a workflow exist for a commit: 0 when the
# repository has no such workflow (HTTP 404: a fallback workflow not yet on
# main), which is an answer, not an error.
fetch_dispatched() { # workflow sha
  local out rc=0 err
  err=$(mktemp "$gate_tmp/err.XXXXXX")
  out=$(gh_call text api "repos/$repo/actions/workflows/$1/runs?event=workflow_dispatch&head_sha=$2&per_page=1" --jq .total_count 2>"$err") || rc=$?
  if [ "$rc" -eq 0 ]; then
    rm -f "$err"
    printf '%s\n' "$out"
    return 0
  fi
  if grep -q 'HTTP 404' "$err"; then
    rm -f "$err"
    echo 0
    return 0
  fi
  cat "$err" >&2
  rm -f "$err"
  return "$rc"
}

# A workflow run's `<status> <run_attempt>`.
fetch_run() { gh_call text api "repos/$repo/actions/runs/$1" --jq '"\(.status) \(.run_attempt)"'; }

# A commit's newest Queue run (queue.yml), found by head_sha as the hosted
# fallbacks' runs are: `<status> <id> <run_attempt> <conclusion> <url>`, or
# nothing when it has none yet.
fetch_queue_run() {
  gh_call text api "repos/$repo/actions/workflows/queue.yml/runs?head_sha=$1&per_page=1" \
    --jq '.workflow_runs[0] // empty | "\(.status) \(.id) \(.run_attempt) \(.conclusion) \(.html_url)"'
}

# A job's annotation messages, one per line (a job is a check run of the
# same id).
fetch_annotations() { gh_call text api "repos/$repo/check-runs/$1/annotations?per_page=100" --jq '.[].message'; }

# A workflow run's `<status> <event>`.
fetch_run_event() { gh_call text api "repos/$repo/actions/runs/$1" --jq '"\(.status) \(.event)"'; }

# A run's jobs, the latest attempt of each, as one {"jobs": [...]}.
fetch_run_jobs() {
  gh_call text api --paginate "repos/$repo/actions/runs/$1/jobs?filter=latest&per_page=100" --jq '.jobs[]' | jq -s '{jobs: .}'
}

# Re-runs a run's failed and cancelled jobs and those that need them (what
# `gh run rerun --failed` asks for).
rerun_failed() { gh_call text api -X POST "repos/$repo/actions/runs/$1/rerun-failed-jobs" >/dev/null; }

dispatch() { # workflow branch sha [key=value...]
  local wf=$1 branch=$2 sha=$3 args=()
  shift 3
  local kv
  for kv in "$@"; do args+=(-f "$kv"); done
  gh_call text workflow run "$wf" --repo "$repo" --ref "$branch" -f "ref=$sha" "${args[@]}" >/dev/null
}

# The follow-up pass: merge-gate-recheck.yml runs merge-gate.yml in 3 minutes.
# The pass's last call, so past the deadline too.
dispatch_recheck() {
  deadline=0
  gh_call text workflow run merge-gate-recheck.yml --repo "$repo" --ref "$default_branch" >/dev/null
}

post() { # sha state description url
  gh_call text api -X POST "repos/$repo/statuses/$1" -f "context=$context" -f "state=$2" \
    -f "description=${3:0:140}" -f "target_url=$4" >/dev/null
}

now() { date -u +%s; }

# --- decisions (pure: JSON or text in, one line out) ----------------------------

# The areas of a file list (one path per line on stdin), by the table given
# as text, or main's.
areas_of() { # [table-text]
  local table=${1:-}
  [ -n "$table" ] || table=$(cat scripts/ci-areas.txt)
  tr '\n' '\0' | changed_areas "$table"
}

# Whether the failing job $1 of a run never produced a result, from the
# run's jobs JSON on stdin (fetch_run_jobs): `infra <runner OS>` when the job
# got no runner and started no step; `cancelled <runner OS> <job-id>...`
# when it, or some of the jobs below, was cancelled after it started (the
# caller reads their annotations: a timeout is a result); the same for a
# summary whose failing parts (its caller's jobs named after it:
# `detection / ci/detection (2)` for `detection / ci/detection`,
# `ci/ios-release` for `ci/ios`) are all such jobs; `superseded` when the
# job is no longer among the latest (a re-run replaced it); else `result`.
# A job that timed out itself, or a part that failed outright, makes it a
# result.
# shellcheck disable=SC2016 # jq variables, not shell ones
jobs_defs='
    def caller: (capture("^(?<p>.* / )") // {p: ""}).p;
    def base: sub("^.* / "; "");
    def bad: .conclusion != null and .conclusion != "success" and .conclusion != "skipped";
    def never: bad and ((.runner_name // "") == "")
      and ([.steps[]? | select(.status != "queued" and .conclusion != "skipped")] | length == 0);
    def cut: .conclusion == "cancelled" and (never | not);
    def os: [(.labels // [])[] | ascii_downcase]
      | if any(.[]; test("macos")) then "macOS"
        elif any(.[]; test("windows")) then "Windows"
        elif any(.[]; test("ubuntu|linux")) then "Linux"
        else "hosted" end;
    def kind: [.[] | select(cut)] as $cut
      | if ($cut | length) == 0 then "infra \(.[0] | os)"
        else "cancelled \($cut[0] | os) \($cut | map(.id | tostring) | join(" "))" end;'
failure_kind() { # job-id
  jq -r --argjson j "$1" "$jobs_defs"'
    ([.jobs[] | select(.id == $j)] | first) as $job
    | if $job == null then "superseded"
      elif $job.conclusion == "timed_out" then "result"
      else ($job.name | caller) as $c | ($job.name | base | sub(" \\(.*$"; "")) as $stem
        | [.jobs[] | select(.id != $j and (.name | caller) == $c and (.name | base | startswith($stem)) and bad)] as $parts
        | if ($job | never or cut) then
            if all($parts[]; never or cut) then [$job] + $parts | kind else "result" end
          elif ($parts | length) > 0 and all($parts[]; never or cut) then $parts | kind
          else "result" end
      end'
}

# The failing parts of a missing check $1 (`ci/ios`: `ios / ci/ios (tests)`,
# `ios / ci/ios-release`) from a run's jobs JSON on stdin: `result <url>`
# when one failed outright or timed out, else `cut <job-id>...`, those
# cancelled after they started (none, when every part never ran or passed).
missing_parts() { # name
  jq -r --arg n "$1" "$jobs_defs"'
    [.jobs[] | select((.name | base | startswith($n)) and bad)] as $parts
    | ([$parts[] | select((never or cut) | not)] | first) as $real
    | if $real then "result \($real.html_url)"
      else "cut \([$parts[] | select(cut) | .id | tostring] | join(" "))" end'
}

# Whether a job that was cancelled after it started hit its own
# timeout-minutes, by its annotations on stdin: a re-run would most likely
# time out again.
timed_out() { grep -q 'has exceeded the maximum execution time'; }

# A job's verdict from check-runs JSON on stdin: the newest run of each
# check named $1, alone or after a caller's "<job> / " (a called workflow's
# job); a name with its caller (`detection-head / ci/detection`) matches
# only that caller's. Prints `<verdict> <url> <finished-epoch>`: success (every one
# concluded success), failure (one concluded anything but success or
# skipped: stale and neutral prove nothing), skipped (all skipped: not
# affected), pending or missing; `-` and 0 when there is nothing to name.
check_verdict() { # name
  jq -r --arg n "$1" '
    def base: .name | sub("^.* / "; "");
    def t: if .completed_at then (.completed_at | fromdateiso8601) else 0 end;
    [.check_runs[] | select(base == $n or .name == $n)] | group_by(.name) | map(max_by(.id)) as $runs
    | ($runs | map(select(.status == "completed" and .conclusion != "success" and .conclusion != "skipped"))) as $bad
    | if ($runs | length) == 0 then "missing - 0"
      elif ($bad | length) > 0 then ($bad | min_by(t) | "failure \(.html_url) \(t)")
      elif any($runs[]; .status != "completed") then "pending - 0"
      elif all($runs[]; .conclusion == "skipped") then "skipped - 0"
      else ($runs | map(select(.conclusion == "success")) | max_by(t) | "success \(.html_url) \(t)") end'
}

# A status context's latest state, its time and its URL from combined-status
# JSON on stdin: `<state> <epoch-or-0> <url>`, or `none 0 -`.
status_of() { # context
  jq -r --arg c "$1" '
    ([.statuses[]? | select(.context == $c)] | first) as $s
    | if $s == null then "none 0 -" else "\($s.state) \($s.updated_at // $s.created_at | fromdateiso8601) \($s.target_url // "-")" end'
}

# The detection gate on a recognition-area PR head (ci.yml's detection-head
# job): its summary, and the caller job alone when it was skipped.
head_detection_check='detection-head / ci/detection'
head_detection_job=detection-head

# The workflow run of a finished check's verdict line (`<verdict> <url> ...`,
# a job URL), or nothing.
run_of() { # verdict-line
  local url
  read -r _ url _ <<<"$1"
  case "$url" in */actions/runs/*/job/*) ;; *) return 0 ;; esac
  url=${url#*/actions/runs/}
  echo "${url%%/*}"
}

# Whether a recognition-area head's missing detection run is not needed,
# from its check-runs JSON and fast-tier verdict line: `yes` when the PR's
# CI skipped the job (its copy of the table, the PR's, said no, or its areas
# job failed), or finished without it (a ci.yml from before the head runs);
# the queue runs the gate anyway. Only a pull_request run counts: a manual
# run of ci.yml on the same commit never runs it (Codex review on #625).
# Else `no`; fails when GitHub could not be read.
detection_exempt() { # checks-json fast-verdict
  local skip run info
  skip=$(jq -r --arg n "$head_detection_job" '[.check_runs[] | select(.name == $n)] | max_by(.id) // empty | "\(.conclusion) \(.html_url)"' <<<"$1")
  case "$skip" in
    "skipped "*)
      run=$(run_of "$skip")
      if [ -n "$run" ]; then
        info=$(fetch_run_event "$run") || return 1
        if [ "${info#* }" = pull_request ]; then
          echo yes
          return 0
        fi
      fi
      ;;
  esac
  run=$(run_of "$2")
  if [ -n "$run" ]; then
    info=$(fetch_run_event "$run") || return 1
    if [ "$info" = "completed pull_request" ]; then
      echo yes
      return 0
    fi
  fi
  echo no
}

# The merge-gate state for a PR head: `<state>|<description>`. A
# recognition-area PR passes a detection verdict and its number; without
# one, or with it skipped, only the fast tier counts.
head_gate() { # areas fast-verdict [detection-verdict pr]
  local areas=$1 verdict=$2 det=${3:-} pr=${4:-} why fast_bad='' fast_short det_bad='' det_short
  if [ -z "$areas" ]; then
    echo "success|Docs only: no job runs"
    return 0
  fi
  case "$det" in
    '' | skipped*)
      case "$verdict" in
        success*) echo "success|Fast tier passed (unit, scripts); the full suite runs in the merge queue" ;;
        failure*) echo "failure|Fast tier failed: fast (ubuntu)" ;;
        # Never ran, out of re-runs: `infra <why>`, underscores for spaces.
        infra*) echo "failure|Fast tier did not run (${verdict#infra }): fast (ubuntu)" | tr _ ' ' ;;
        *) echo "pending|Waiting for the fast tier: fast (ubuntu)" ;;
      esac
      return 0
      ;;
  esac
  # Each failure in full, and short for when both failed (140 characters).
  case "$verdict" in
    failure*)
      fast_bad="Fast tier failed: fast (ubuntu)"
      fast_short="fast (ubuntu)"
      ;;
    infra*)
      why=${verdict#infra }
      fast_bad="Fast tier did not run (${why//_/ }): fast (ubuntu)"
      fast_short="fast (ubuntu): ${why//_/ }"
      ;;
  esac
  case "$det" in
    # The shards upload the stale fixtures' snapshots, regenerated.
    failure*)
      det_bad="Detection gate failed: $head_detection_check; regenerated snapshots: just snapshots-fetch $pr"
      det_short="$head_detection_check (just snapshots-fetch $pr)"
      ;;
    infra*)
      why=${det#infra }
      det_bad="Detection gate did not run (${why//_/ }): $head_detection_check"
      det_short="$head_detection_check: ${why//_/ }"
      ;;
  esac
  if [ -n "$fast_bad" ] && [ -n "$det_bad" ]; then
    echo "failure|Failed: $fast_short; $det_short"
    return 0
  elif [ -n "$fast_bad$det_bad" ]; then
    echo "failure|$fast_bad$det_bad"
    return 0
  fi
  case "${verdict%% *} ${det%% *}" in
    "success success") echo "success|Fast tier and detection gate passed; the full suite runs in the merge queue" ;;
    "success "*) echo "pending|Fast tier passed; waiting for the detection gate: $head_detection_check" ;;
    *" success") echo "pending|Detection gate passed; waiting for the fast tier: fast (ubuntu)" ;;
    *) echo "pending|Waiting for the fast tier and the detection gate: fast (ubuntu), $head_detection_check" ;;
  esac
}

# PR numbers from commit subjects on stdin: squash merges end "(#N)",
# merge commits say "Merge pull request #N".
group_prs() {
  sed -nE 's/.*\(#([0-9]+)\)$/\1/p; s/^Merge pull request #([0-9]+).*/\1/p' | sort -u
}

# A PC job's state (windows or android): `<verdict> <where> <url>`, where the
# verdict is success, failure, pending or fallback (run it hosted now). The
# PC's result and a hosted fallback's: the one that finished first counts; a
# PC `error` (the runner's own trouble) falls back instead of failing.
pc_job() { # pc-status-line hosted-verdict hosted-dispatched third-party commit-age-min now
  local state at url hv hurl hat dispatched=$3 third=$4 age=$5 t=$6 pc_done=false hosted_done=false
  read -r state at url <<<"$1"
  read -r hv hurl hat <<<"$2"
  case "$state" in success | failure) pc_done=true ;; esac
  case "$hv" in success | failure) hosted_done=true ;; esac
  if $pc_done && $hosted_done; then
    if [ "${hat:-0}" -lt "${at:-0}" ]; then pc_done=false; else hosted_done=false; fi
  fi
  if $pc_done; then
    echo "$state PC $url"
    return 0
  fi
  if $hosted_done; then
    echo "$hv hosted $hurl"
    return 0
  fi
  if [ "$dispatched" -gt 0 ] || [ "$hv" = pending ]; then
    echo "pending hosted -"
    return 0
  fi
  if [ "$third" = true ]; then
    echo "fallback hosted third-party"
    return 0
  fi
  # A runner error is the PC's trouble, not a test result.
  if [ "$state" = error ]; then
    echo "fallback hosted pc-error"
    return 0
  fi
  if [ "$state" = none ]; then
    if [ "$age" -ge "$silent_min" ]; then echo "fallback hosted silent"; else echo "pending PC -"; fi
    return 0
  fi
  # Pending: its heartbeat.
  if [ "$at" -gt 0 ] && [ $(((t - at) / 60)) -ge "$silent_min" ]; then
    echo "fallback hosted stale"
  else
    echo "pending PC $url"
  fi
}

# Whether a hosted fallback workflow can be dispatched: 0 yes, 1 no (GitHub
# has none, or it is disabled: sets no_hosted to why), 2 could not tell.
hosted_ready() { # workflow
  local out rc=0
  out=$(fetch_workflow_state "$1" 2>&1) || rc=$?
  out=$(tr -d '\r' <<<"$out")
  if [ "$rc" -eq 0 ]; then
    [ "$out" != active ] || return 0
    no_hosted="$1 is $out"
    return 1
  fi
  case "$out" in
    *"HTTP 404"*)
      no_hosted="no hosted job"
      return 1
      ;;
  esac
  return 2
}

# The newest time the PC posted ci/$1 on main or a queue commit, in epoch
# seconds (0 when never): whether its runner is up, busy with another group.
# False when a commit's statuses could not be read: the PC may be busy there.
pc_seen() { # job main queue-lines
  local job=$1 latest=0 sha json line at
  for sha in $2 $(awk '{ print $2 }' <<<"$3"); do
    json=$(fetch_status "$sha") || return 1
    line=$(status_of "ci/$job" <<<"$json") || return 1
    read -r _ at _ <<<"$line"
    [ "${at:-0}" -le "$latest" ] || latest=$at
  done
  echo "$latest"
}

# Why a PC job has nowhere to run, as one word (underscores read as spaces in
# the gate's description), or nothing when the PC may still run it.
nowhere() { # fallback-reason job main queue-lines now
  case "$1" in
    third-party) echo "${no_hosted// /_};_third-party_code_never_runs_on_the_PC" ;;
    pc-error) echo "${no_hosted// /_};_PC_runner_error" ;;
    stale) echo "${no_hosted// /_};_PC_heartbeat_stopped" ;;
    *)
      local seen
      # Unknown: decided at the next pass.
      seen=$(pc_seen "$2" "$3" "$4") || return 0
      [ $((($5 - seen) / 60)) -lt "$silent_min" ] || echo "${no_hosted// /_};_PC_silent_${silent_min}_min"
      ;;
  esac
}

# The merge-gate state for a queue commit from job lines on stdin
# (`<job> <verdict> <where> <url>`): `<state>|<description>|<url>`.
queue_gate() {
  awk -v run="$1" '
    NF == 0 { next }
    {
      job = $1; v = $2; where = $3; url = $4
      gsub(/_/, " ", where)
      if (v == "success") ok = ok (ok ? ", " : "") job (where != "" && where != "-" ? " (" where ")" : "")
      else if (v == "skipped") na = na (na ? ", " : "") job
      else if (v == "failure") { bad = bad (bad ? ", " : "") job (where != "" && where != "-" ? " (" where ")" : ""); if (badurl == "" && url != "-") badurl = url }
      else wait = wait (wait ? ", " : "") job
    }
    END {
      if (bad != "") { print "failure|Failed: " bad (ok ? "; passed: " ok : "") "|" (badurl ? badurl : run); exit }
      if (wait != "") { print "pending|Waiting: " wait (ok ? "; passed: " ok : "") "|" run; exit }
      if (ok == "" && na == "") { print "success|Docs only: no job runs|" run; exit }
      print "success|Passed: " ok (na ? "; not affected: " na : "") "|" run
    }'
}

# --- one pass -------------------------------------------------------------------

# Set when this pass wants a follow-up pass (why), besides an error.
recheck=''

# Posts merge-gate on $1 unless it already says the same.
settle() { # sha state description url current-status-json
  local cur
  cur=$(jq -r --arg c "$context" '([.statuses[]? | select(.context == $c)] | first) as $s | "\($s.state // "none")|\($s.description // "")"' <<<"$5")
  if [ "$cur" = "$2|${3:0:140}" ]; then return 0; fi
  post "$1" "$2" "$3" "$4" && log "${1:0:7}: $2: $3"
}

# A commit or PR left undecided this pass; the caller ends red.
skipped() { log "$1: skipped: could not read $2"; }

# A hosted check's verdict from check-runs JSON ($2): check_verdict's line,
# with a failure looked into. When the jobs behind it never ran:
# `infra <run> <attempt> <why> <url>` (resolve_infra decides; why is
# `no_macOS_runner` or `cancelled_on_macOS`), or pending while that run is
# still going (a re-run, or other jobs; a re-run needs it finished). Fails
# when GitHub could not be read.
hosted_verdict() { # name checks-json
  local line v url run job jobs kind info state attempt why os ids id notes
  line=$(check_verdict "$1" <<<"$2")
  read -r v url _ <<<"$line"
  case "$v $url" in
    "failure "*/actions/runs/*/job/*) ;;
    *)
      echo "$line"
      return 0
      ;;
  esac
  run=${url#*/actions/runs/}
  job=${run#*/job/}
  run=${run%%/*}
  job=${job%%[!0-9]*}
  jobs=$(fetch_run_jobs "$run") || return 1
  kind=$(failure_kind "$job" <<<"$jobs") || return 1
  case "$kind" in
    infra*) why="no_${kind#infra }_runner" ;;
    # Cancelled after it started: infrastructure, unless one hit its own
    # timeout-minutes.
    cancelled*)
      read -r _ os ids <<<"$kind"
      for id in $ids; do
        notes=$(fetch_annotations "$id") || return 1
        if timed_out <<<"$notes"; then
          echo "$line"
          return 0
        fi
      done
      why="cancelled_on_$os"
      ;;
    # The check runs predate a re-run that replaced the job: wait for it.
    superseded)
      echo "pending - 0"
      return 0
      ;;
    *)
      echo "$line"
      return 0
      ;;
  esac
  info=$(fetch_run "$run") || return 1
  read -r state attempt <<<"$info"
  if [ "$state" != completed ]; then
    echo "pending - 0"
  else
    echo "infra $run ${attempt:-1} $why $url"
  fi
}

# A queue commit's hosted check that is missing, from the commit's Queue run
# line (fetch_queue_run, or nothing): pending while the run has not
# completed (its summary job is created once its parts end); then the run's
# jobs never all ran (#635: a part cancelled by an internal error, its
# summary never created), so `infra <run> <attempt> did_not_run <url>`; and
# `missed <url>` when the run succeeded without it, with nothing to re-run.
missing_verdict() { # queue-run-line
  local state id attempt conclusion url
  read -r state id attempt conclusion url <<<"$1"
  if [ "$state" != completed ]; then
    echo "pending - 0"
  elif [ "$conclusion" = success ]; then
    echo "missed $url"
  else
    echo "infra $id ${attempt:-1} did_not_run $url"
  fi
}

# Whether a missing check's parts in its completed run $2 hold a result (a
# part that failed outright, or hit its own timeout-minutes): `failure <url>
# 0`, a real failure, or nothing. Fails when GitHub could not be read.
missing_result() { # name run
  local jobs out id notes
  jobs=$(fetch_run_jobs "$2") || return 1
  out=$(missing_parts "$1" <<<"$jobs") || return 1
  case "$out" in
    result*)
      echo "failure ${out#result } 0"
      return 0
      ;;
  esac
  for id in ${out#cut}; do
    notes=$(fetch_annotations "$id") || return 1
    if timed_out <<<"$notes"; then
      jq -r --argjson i "$id" '.jobs[] | select(.id == $i) | "failure \(.html_url) 0"' <<<"$jobs"
      return 0
    fi
  done
}

# Job lines on stdin (`<job> <verdict> <where> <url>`, and
# `<job> infra <run> <attempt> <why> <url>` from hosted_verdict or
# missing_verdict) with the infra ones decided: a run with re-runs left is
# re-run (once a pass) and its jobs wait; a run without fails them as
# infrastructure. Beside a job that failed for real they only wait: the
# commit fails anyway.
resolve_infra() { # what (for the log)
  local lines job v run attempt why url real=false done_runs=' '
  lines=$(cat)
  if grep -q '^[^ ]* failure ' <<<"$lines"; then real=true; fi
  while read -r job v run attempt why url; do
    [ -n "$job" ] || continue
    if [ "$v" != infra ]; then
      echo "$job $v $run $attempt"
    elif $real; then
      echo "$job pending - -"
    elif [ "$attempt" -le "$reruns" ]; then
      if [[ $done_runs != *" $run "* ]]; then
        done_runs+="$run "
        if rerun_failed "$run"; then
          log "$1 $job: ${why//_/ } (attempt $attempt); re-running run $run's failed jobs"
        else
          log "$1 $job: could not re-run run $run"
        fi
      fi
      echo "$job pending - -"
    else
      log "$1 $job: ${why//_/ } in $attempt attempts"
      echo "$job failure ${why},_${attempt}_attempts $url"
    fi
  done <<<"$lines"
}

# The area table a queue commit brings, read from the commit as queue.yml
# and the PC read it; main's when GitHub cannot give it.
commit_table() { # sha
  local table
  if table=$(fetch_file "$1" scripts/ci-areas.txt 2>/dev/null) && [ -n "$table" ]; then
    printf '%s\n' "$table"
  else
    log "${1:0:7}: could not read its scripts/ci-areas.txt; using main's"
    cat scripts/ci-areas.txt
  fi
}

# Warns, in the log and as an annotation, when PR $1 is green with auto-merge
# on but not in the merge queue $enqueue_min minutes later: GitHub missed it
# (#613). No repair; until then it asks for a follow-up pass to look again.
missed_enqueue() { # pr gate-state auto-merge-since in-queue lint-verdict status-json now
  local lv lint_t gs gate_t since min
  [ "$2" = success ] && [ "${3:-0}" -gt 0 ] && [ "$4" = false ] || return 0
  read -r lv _ lint_t <<<"$5"
  [ "$lv" = success ] || return 0
  read -r gs gate_t _ <<<"$(status_of "$context" <<<"$6")"
  # Green from this pass on.
  [ "$gs" = success ] || gate_t=$7
  since=$3
  [ "${lint_t:-0}" -le "$since" ] || since=$lint_t
  [ "${gate_t:-0}" -le "$since" ] || since=$gate_t
  min=$((($7 - since) / 60))
  if [ "$min" -ge "$enqueue_min" ]; then
    log "#$1: green with auto-merge on for $min min, but not in the merge queue"
    echo "::warning title=Missed enqueue::#$1 is green with auto-merge on but not in the merge queue after $min min. Turn auto-merge off and on: gh pr merge $1 --disable-auto, then gh pr merge $1 --auto."
  else
    recheck="#$1 waits for the merge queue"
  fi
}

heads_pass() {
  local list n draft head auto queued files areas checks verdict det detects exempt lines job lint line state desc status url t v where rc=0
  list=$(fetch_heads) || {
    log "could not list PRs"
    return 1
  }
  t=$(now)
  while read -r n draft head auto queued; do
    if out_of_time; then
      rc=1
      break
    fi
    if [ -z "$head" ] || [ "$draft" != false ]; then continue; fi
    files=$(fetch_pr_files "$n") || {
      skipped "#$n" "its files"
      rc=1
      continue
    }
    # Too many files to tell: a head run is not waited for, but counts when
    # it exists (below); the queue runs the gate anyway.
    detects=false
    if [ "$(wc -l <<<"$files")" -ge 3000 ]; then
      areas='core desktop recognition ios android ci'
    else
      areas=$(areas_of <<<"$files")
      if tr '\n' '\0' <<<"$files" | head_detection "$(cat scripts/ci-areas.txt)"; then detects=true; fi
    fi
    checks=$(fetch_check_runs "$head") || {
      skipped "#$n" "the check runs of ${head:0:7}"
      rc=1
      continue
    }
    verdict=$(hosted_verdict "fast (ubuntu)" "$checks") || {
      skipped "#$n" "the run behind fast (ubuntu)"
      rc=1
      continue
    }
    # A head detection run that exists always counts, whatever main's table
    # says: the PR's CI decided with the full diff and the PR's own table.
    det=$(hosted_verdict "$head_detection_check" "$checks") || {
      skipped "#$n" "the run behind $head_detection_check"
      rc=1
      continue
    }
    case "$det" in
      missing*)
        if ! $detects; then
          det=''
        else
          exempt=$(detection_exempt "$checks" "$verdict") || {
            skipped "#$n" "the CI run of ${head:0:7}"
            rc=1
            continue
          }
          [ "$exempt" != yes ] || det="skipped - 0"
        fi
        ;;
    esac
    # Jobs that never ran, decided together (both are in the PR's CI run):
    # re-run, so pending, or out of re-runs, `infra <why>`.
    if [[ $verdict == infra* || $det == infra* ]]; then
      lines="fast $verdict"
      [ -z "$det" ] || lines+=$'\n'"detection $det"
      while read -r job v where _; do
        if [ "$v" = failure ]; then v="infra $where"; else v="pending - 0"; fi
        case "$job" in
          fast) [[ $verdict != infra* ]] || verdict=$v ;;
          detection) [[ $det != infra* ]] || det=$v ;;
        esac
      done <<<"$(printf '%s\n' "$lines" | resolve_infra "#$n")"
    fi
    # A running fast tier or head detection (or a re-run): look again even if
    # its finish is missed. Not for one that never started: CI's own run
    # starts the next pass.
    case "${verdict%% *} ${det%% *}" in
      pending* | *" pending") [ -z "$areas" ] || recheck="#$n waits for its head checks" ;;
    esac
    line=$(head_gate "$areas" "$verdict" "$det" "$n")
    state=${line%%|*}
    desc=${line#*|}
    # A failing head detection links to its run, where the snapshots are.
    url=$run_url
    case "$det" in "failure https://"*) [ "$state" != failure ] || read -r _ url _ <<<"$det" ;; esac
    status=$(fetch_status "$head") || {
      skipped "#$n" "the statuses of ${head:0:7}"
      rc=1
      continue
    }
    settle "$head" "$state" "$desc" "$url" "$status" || rc=1
    lint=$(check_verdict "lint (ubuntu)" <<<"$checks")
    missed_enqueue "$n" "$state" "$auto" "$queued" "$lint" "$status" "$t"
  done <<<"$list"
  return "$rc"
}

queue_pass() {
  local t main branch sha cmp areas jobs checks status made age third prs pr kind cross job hv line state desc url v where u
  local queue why ready no_hosted='' info disp pcline rc=0 lines wf extra hinfra qrun
  t=$(now)
  main=$(fetch_main) || main=''
  [ -n "$main" ] || {
    log "could not read main"
    return 1
  }
  queue=$(fetch_queue) || {
    log "could not list the queue"
    return 1
  }
  while read -r branch sha; do
    if out_of_time; then
      rc=1
      break
    fi
    [ -n "$sha" ] || continue
    cmp=$(fetch_compare "$main" "$sha") || {
      skipped "${sha:0:7}" "what it brings onto main"
      rc=1
      continue
    }
    if [ "$(jq '.files | length' <<<"$cmp")" -ge 300 ]; then
      areas='core desktop recognition ios android ci'
    else
      areas=$(jq -r '.files[] | .filename, (.previous_filename // empty)' <<<"$cmp" | areas_of "$(commit_table "$sha")")
    fi
    jobs=$(needed_jobs "$areas")
    checks=$(fetch_check_runs "$sha") || {
      skipped "${sha:0:7}" "its check runs"
      rc=1
      continue
    }
    status=$(fetch_status "$sha") || {
      skipped "${sha:0:7}" "its statuses"
      rc=1
      continue
    }
    made=$(fetch_commit_time "$sha") || made=''
    case "$made" in '' | *[!0-9]*)
      skipped "${sha:0:7}" "its commit time"
      rc=1
      continue
      ;;
    esac
    age=$(((t - made) / 60))
    third=false
    prs=$(jq -r '.commits[].commit.message | split("\n")[0]' <<<"$cmp" | group_prs)
    [ -n "$prs" ] || third=true
    for pr in $prs; do
      info=$(fetch_pr "$pr") || {
        skipped "${sha:0:7}" "PR #$pr"
        rc=1
        continue 2
      }
      read -r kind cross <<<"$info"
      if [ "$kind" != User ] || [ "$cross" != false ]; then third=true; fi
    done
    lines='' qrun=''
    for job in $jobs; do
      case "$job" in
        macos | ios | detection | scripts)
          line=$(hosted_verdict "ci/$job" "$checks") || {
            skipped "${sha:0:7}" "the run behind ci/$job"
            rc=1
            continue 2
          }
          read -r v u _ <<<"$line"
          # Missing: the commit's Queue run decides (#635), read once. The
          # first time it has completed, the check runs are read again, in
          # case the job ended between the two reads.
          if [ "$v" = missing ] && [ -z "$qrun" ]; then
            qrun=$(fetch_queue_run "$sha") || {
              skipped "${sha:0:7}" "its Queue run"
              rc=1
              continue 2
            }
            qrun=${qrun:-none}
            if [ "${qrun%% *}" = completed ]; then
              if ! checks=$(fetch_check_runs "$sha") || ! line=$(hosted_verdict "ci/$job" "$checks"); then
                skipped "${sha:0:7}" "the run behind ci/$job"
                rc=1
                continue 2
              fi
              read -r v u _ <<<"$line"
            fi
          fi
          # A completed run without it: infrastructure, unless one of its
          # parts failed outright or timed out.
          if [ "$v" = missing ]; then
            line=$(missing_verdict "$qrun")
            read -r v u _ <<<"$line"
            if [ "$v" = infra ]; then
              read -r _ u _ <<<"$line"
              u=$(missing_result "ci/$job" "$u") || {
                skipped "${sha:0:7}" "the run behind ci/$job"
                rc=1
                continue 2
              }
              [ -z "$u" ] || line=$u
              read -r v u _ <<<"$line"
            fi
          fi
          case "$v" in
            success) where=hosted u=- ;;
            failure) where=hosted ;;
            infra) where='' ;;
            # The run succeeded without it: nothing to re-run.
            missed) v=failure where=did_not_run ;;
            *) where=- u=- ;;
          esac
          if [ "$v" = infra ]; then lines+="$job $line"$'\n'; else lines+="$job $v $where $u"$'\n'; fi
          ;;
        windows | android)
          wf=ci-desktop.yml extra=os=windows
          [ "$job" = windows ] || wf=ci-android.yml extra=
          pcline=$(status_of "ci/$job" <<<"$status")
          hv=$(hosted_verdict "ci/$job (hosted)" "$checks") || {
            skipped "${sha:0:7}" "the run behind ci/$job (hosted)"
            rc=1
            continue 2
          }
          # A hosted run that never ran is no result: pc_job waits on it,
          # and resolve_infra decides it with the commit's other jobs.
          hinfra=''
          case "$hv" in
            infra*)
              hinfra=$hv
              hv="pending - 0"
              ;;
          esac
          disp=$(fetch_dispatched "$wf" "$sha") || {
            skipped "${sha:0:7}" "the hosted $job runs"
            rc=1
            continue 2
          }
          line=$(pc_job "$pcline" "$hv" "$disp" "$third" "$age" "$t")
          read -r v where u <<<"$line"
          if [ -n "$hinfra" ] && [ "$v $where" = "pending hosted" ]; then
            lines+="$job $hinfra"$'\n'
            continue
          fi
          if [ "$v" = fallback ]; then
            ready=0
            hosted_ready "$wf" || ready=$?
            case "$ready" in
              0)
                # shellcheck disable=SC2086 # extra is one key=value or nothing
                if dispatch "$wf" "$branch" "$sha" $extra; then
                  log "${sha:0:7} $job: hosted fallback dispatched ($u)"
                else
                  log "${sha:0:7} $job: could not dispatch $wf"
                  rc=1
                fi
                v=pending u=-
                ;;
              1)
                why=$(nowhere "$u" "$job" "$main" "$queue" "$t")
                if [ -n "$why" ]; then
                  log "${sha:0:7} $job: nowhere to run: ${why//_/ }"
                  v=failure where=$why u=-
                else
                  v=pending where=PC u=-
                fi
                ;;
              *)
                log "${sha:0:7} $job: could not read $wf; deciding at the next pass"
                rc=1
                v=pending u=-
                ;;
            esac
          fi
          lines+="$job $v $where ${u:--}"$'\n'
          ;;
      esac
    done
    lines=$(printf '%s' "$lines" | resolve_infra "${sha:0:7}")
    line=$(printf '%s\n' "$lines" | queue_gate "$run_url")
    state=${line%%|*}
    desc=${line#*|}
    url=${desc##*|}
    desc=${desc%|*}
    [ "$state" != pending ] || recheck="${sha:0:7} is pending"
    settle "$sha" "$state" "$desc" "$url" "$status" || rc=1
  done <<<"$queue"
  return "$rc"
}

# Runs for queue commits the queue no longer has: GitHub deletes a branch
# under gh-readonly-queue/ when it drops or rebuilds a group, but the runs
# for it go on and hold the account's five macOS runners (2026-11-03). A
# trial branch under ci-shadow/ is left alone until it is deleted too. A run
# that finished meanwhile is not cancelled, silently.
stale_pass() {
  local current runs id branch n=0
  ! out_of_time || return 1
  # Runs first: a branch exists before its runs start, so a group created
  # between the two reads is in the queue list, never taken for stale.
  runs=$(fetch_active_runs) || {
    log "could not list the active runs"
    return 1
  }
  current=$(fetch_queue | awk '{ print $1 }') || {
    log "could not list the queue"
    return 1
  }
  while read -r id branch; do
    [ -n "$id" ] || continue
    case "$branch" in gh-readonly-queue/main/* | ci-shadow/main/*) ;; *) continue ;; esac
    if ! grep -qxF "$branch" <<<"$current"; then
      if cancel_run "$id"; then
        log "cancelled run $id: $branch left the queue"
        n=$((n + 1))
      fi
    fi
  done <<<"$runs"
  [ "$n" -eq 0 ] || log "cancelled $n run(s) of queue commits that left the queue"
}

# One pass: PR heads, queue commits, stale runs. Red when anything could not
# be read; then, or when something waits, a follow-up pass in 3 minutes.
gate_main() {
  local rc=0 failed
  : >"$gate_tmp/errors"
  rm -f "$gate_tmp/late"
  recheck=''
  deadline=$(($(date -u +%s) + budget))
  heads_pass || rc=1
  queue_pass || rc=1
  stale_pass || rc=1
  if [ -s "$gate_tmp/errors" ]; then
    failed=$(sort -u "$gate_tmp/errors" | awk '{ printf "%s%s", (NR > 1 ? "; " : ""), $0 }')
    echo "::error title=GitHub calls failed::$failed"
    rc=1
  fi
  [ "$rc" -eq 0 ] || recheck="this pass hit an error"
  if [ -n "$recheck" ]; then
    if dispatch_recheck; then
      log "follow-up pass in 3 minutes: $recheck"
    else
      log "could not dispatch merge-gate-recheck.yml"
      rc=1
    fi
  fi
  return "$rc"
}

if [ -z "${MERGE_GATE_SOURCED:-}" ]; then
  command -v jq >/dev/null || die "jq not found"
  rc=0
  gate_main || rc=1
  exit "$rc"
fi
