#!/usr/bin/env bash
# Usage: scripts/local-ci-fallback.sh
#
# The hosted fallback for local CI (ADR 0082, ADR 0091). When a PR head's
# local/macos or local/windows status is stuck, it dispatches the full hosted
# CI run on the PR branch and, when that run's clippy and test jobs for the
# same OS finish, posts their result under the stuck context, so the merge
# gate (lint (ubuntu) and the four local statuses) stays the same. A stuck
# local/ios or local/android is posted success at once, saying it was not
# tested: there is no hosted mobile job yet (temporary, below).
# .github/workflows/local-ci-fallback.yml runs it every 10 minutes and after
# each CI run, with the workflow's token; it never checks out PR code.
#
# Stuck means, for a PR from a branch in the repository, opened by a user,
# not a draft (the heads the runners test):
#   no status        15 min after the head appeared (its lint (ubuntu) run)
#   "Queued; waiting for ..."   15 min: the runner is up but out of capacity
#   "Running ..."    50 min: each step posts its own status, and the longest
#                    step limit is 40 min, so the runner stopped
#   other pending    45 min ("Queued": a long queue, or the runner is gone)
#   "Queued" behind 2 or more   5 min, for local/macos and local/windows:
#                    overflow. Each runner tests one head at a time; hosted
#                    CI runs many at once (free on a public repository), so
#                    heads that would wait behind two others go there. Ahead
#                    means the same context's other local "Running" heads and
#                    its "Queued" ones posted earlier.
#
# The fallback's own statuses start with "Hosted fallback". A runner that
# comes back still tests such a head, pending or finished, and its status
# replaces the fallback's. The fallback writes only over the status it read,
# re-reading it right before each write, so a runner's result wins. A
# dispatch with no result in 25 min (cancelled or missing) or still queued
# after 60 min is retried, 3 dispatches in all, then the context gets error.
# Hosted CI has no `just mobile`: a local/macos it posts says so.
#
# TEMPORARY (ADR 0091 rollout item 5 replaces it with the hosted iOS and
# Android jobs): a stuck local/ios or local/android gets success "Hosted
# fallback: NOT tested; no hosted iOS job yet (ADR 0091 item 5)", the same
# gap the desktop fallback had for mobile, so a machine that is off never
# blocks merges. A runner that comes back tests the head and replaces it.
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

repo=${LOCAL_CI_REPO:-lawkitt/mdoc}
prefix="Hosted fallback"
contexts=(local/macos local/windows local/ios local/android)
no_status_min=${FALLBACK_NO_STATUS_MIN:-15}
waiting_min=${FALLBACK_WAITING_MIN:-15}
running_min=${FALLBACK_RUNNING_MIN:-50}
queued_min=${FALLBACK_QUEUED_MIN:-45}
overflow_ahead=${FALLBACK_OVERFLOW_AHEAD:-2}
overflow_min=${FALLBACK_OVERFLOW_MIN:-5}
retry_min=${FALLBACK_RETRY_MIN:-25}
queued_retry_min=${FALLBACK_QUEUED_RETRY_MIN:-60}
max_attempts=${FALLBACK_MAX_ATTEMPTS:-3}
adr_url=https://github.com/$repo/blob/main/docs/adr/0091-mobile-ci-and-releases.md

# The hosted runner that stands in for each context: ci.yml's shipped matrix.
hosted_runner() {
  case "$1" in
    local/macos) echo macos-26 ;;
    local/windows) echo windows-latest ;;
    *) return 1 ;;
  esac
}

# The job a mobile context stands for, which nothing hosted runs yet
# (TEMPORARY, ADR 0091 item 5); false for the desktop contexts.
untested_job() {
  case "$1" in
    local/ios) echo iOS ;;
    local/android) echo Android ;;
    *) return 1 ;;
  esac
}

log() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }

# jq for Windows ends lines with CRLF, which leaves a \r in the last field
# (the suite runs in local CI on the PC too).
jq() { command jq "$@" | tr -d '\r'; }

# --- GitHub (replaced by fakes in scripts/test-local-ci-fallback.sh) --------

# shellcheck disable=SC2016 # GraphQL variables, not shell ones
query='query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 50) {
      nodes {
        number isDraft isCrossRepository headRefName headRefOid
        author { __typename }
        commits(last: 1) { nodes { commit {
          committedDate
          status { contexts { context state description createdAt } }
          statusCheckRollup { contexts(first: 100) { nodes {
            __typename ... on CheckRun { name startedAt }
          } } }
        } } }
      }
    }
  }
}'

fetch_prs() { gh api graphql -f owner="${repo%/*}" -f name="${repo#*/}" -f query="$query"; }

fetch_check_runs() { gh api "repos/$repo/commits/$1/check-runs?per_page=100"; }

# The combined status: the latest status of each context on the commit.
fetch_status() { gh api "repos/$repo/commits/$1/status"; }

dispatch() { # branch
  gh workflow run ci.yml --repo "$repo" --ref "$1"
}

post() { # sha context state description url
  gh api -X POST "repos/$repo/statuses/$1" -f "context=$2" -f "state=$3" \
    -f "description=${4:0:140}" -f "target_url=$5" >/dev/null
}

now() { date -u +%s; }

# --- decisions ----------------------------------------------------------------

# One line per tested PR head and context, from fetch_prs' JSON on stdin,
# fields split by the unit separator (a tab is whitespace to read, which
# merges empty fields):
# number, branch, head, context, state (or NONE), description, status time
# and the time the head appeared (epoch seconds; 0 when unknown).
rows() {
  local ctx_json
  ctx_json=$(printf '%s\n' "${contexts[@]}" | jq -R . | jq -sc .)
  jq -r --argjson ctxs "$ctx_json" '
    def epoch: if . == null or . == "" then 0 else fromdateiso8601 end;
    .data.repository.pullRequests.nodes[]
    | select(.isDraft == false and .isCrossRepository == false and .author.__typename == "User")
    | . as $pr
    | (.commits.nodes[0].commit) as $c
    | ([($c.committedDate | epoch)]
        + [$c.statusCheckRollup.contexts.nodes[]?
            | select(.__typename == "CheckRun" and .name == "lint (ubuntu)")
            | (.startedAt | epoch)]
        | max) as $seen
    | $ctxs[] as $ctx
    | ([$c.status.contexts[]? | select(.context == $ctx)] | last) as $s
    | [$pr.number, $pr.headRefName, $pr.headRefOid, $ctx,
        ($s.state // "NONE"), ($s.description // "" | gsub("[\t\n]"; " ")),
        ($s.createdAt | epoch), $seen]
    | map(tostring) | join("\u001f")'
}

# Why the context is stuck, or nothing (exit 1) when it is not. Ahead: the
# heads queued or running before it (overflow, desktop contexts only).
stuck_reason() { # state description status-time seen-time now [ahead]
  local state=$1 desc=$2 at=$3 seen=$4 t=$5 ahead=${6:-0} age limit
  case "$state" in
    NONE)
      age=$(((t - seen) / 60))
      [ "$seen" -gt 0 ] && [ "$age" -ge "$no_status_min" ] || return 1
      echo "no runner status after ${age} min"
      ;;
    PENDING)
      [[ $desc != "$prefix"* ]] || return 1
      age=$(((t - at) / 60))
      case "$desc" in
        Running*) limit=$running_min ;;
        *"waiting for"*) limit=$waiting_min ;;
        *) limit=$queued_min ;;
      esac
      if [ "$age" -lt "$limit" ]; then
        [[ $desc == Queued* ]] && [ "$ahead" -ge "$overflow_ahead" ] && [ "$age" -ge "$overflow_min" ] || return 1
        echo "overflow: queued behind $ahead for ${age} min"
        return 0
      fi
      echo "\"${desc:0:40}\" for ${age} min"
      ;;
    *) return 1 ;;
  esac
}

attempt_of() { # description
  if [[ $1 =~ \(attempt\ ([0-9]+)\) ]]; then echo "${BASH_REMATCH[1]}"; else echo 1; fi
}

# The hosted result for a context from check-runs JSON on stdin, tab
# separated: "success <url>", "failure <job> <url>", "running", "queued" (no
# job has started), or "missing" (a job not there, or one that ended
# cancelled or skipped).
hosted_result() { # context
  local runner
  runner=$(hosted_runner "$1")
  jq -r --arg runner "$runner" '
    def failed: .status == "completed" and (.conclusion == "failure" or .conclusion == "timed_out");
    [.check_runs[] | select(.name == "clippy (\($runner))" or .name == "test (\($runner))")]
    | group_by(.name) | map(max_by(.id)) as $jobs
    | if ($jobs | length) < 2 then "missing"
      elif any($jobs[]; failed) then ([$jobs[] | select(failed)] | first | "failure\t\(.name)\t\(.html_url)")
      elif all($jobs[]; .status == "completed" and .conclusion == "success")
        then ([$jobs[] | select(.name | startswith("test"))] | first | "success\t\(.html_url)")
      elif any($jobs[]; .status == "completed" and .conclusion != "success") then "missing"
      elif any($jobs[]; .status == "in_progress") then "running"
      else "queued" end'
}

# True when the context's latest status on the head is still the one this
# pass read: a runner may have posted since.
unchanged() { # head context state description
  local now_seen
  now_seen=$(fetch_status "$1" | jq -r --arg c "$2" '
    ([.statuses[]? | select(.context == $c)] | first) as $s
    | [($s.state // "none" | ascii_upcase), ($s.description // "" | gsub("[\t\n]"; " "))]
    | join("\u001f")') || return 1
  [ "$now_seen" = "$3"$'\x1f'"$4" ]
}

# Posts only over the status this pass read.
post_over() { # head context old-state old-description state description url
  if unchanged "$1" "$2" "$3" "$4"; then
    post "$1" "$2" "$5" "$6" "$7"
  else
    log "${1:0:7} $2: changed since read; left alone"
    return 1
  fi
}

runs_url() { echo "https://github.com/$repo/actions/workflows/ci.yml?query=branch%3A$1"; }

# Prints, for each line of rows(), the number of heads ahead of it in its
# runner's queue: for a desktop context's local "Queued" status, the same
# context's local "Running" statuses and the "Queued" ones posted earlier;
# 0 for everything else.
ahead_counts() { # rows-lines
  awk -v FS=$'\x1f' '
    { pr[NR] = $1; ctx[NR] = $4; st[NR] = $5; desc[NR] = $6; at[NR] = $7 + 0 }
    END {
      for (i = 1; i <= NR; i++) {
        n = 0
        if ((ctx[i] == "local/macos" || ctx[i] == "local/windows") && st[i] == "PENDING" && desc[i] ~ /^Queued/) {
          for (j = 1; j <= NR; j++) {
            if (j == i || ctx[j] != ctx[i] || st[j] != "PENDING" || pr[j] == pr[i]) continue
            if (desc[j] ~ /^Running/ || (desc[j] ~ /^Queued/ && at[j] < at[i])) n++
          }
        }
        print n
      }
    }' <<<"$1"
}

# --- one pass -------------------------------------------------------------------

tick() {
  local t json lines
  t=$(now)
  json=$(fetch_prs) || die "could not list PRs"
  lines=$(rows <<<"$json")
  [ -n "$lines" ] || {
    log "no PR heads for the runners"
    return 0
  }

  # Claims and retries are collected per PR, then dispatched once: one
  # dispatch serves both contexts (the full matrix), and a second would
  # cancel the first (ci.yml's concurrency group). A PR with a hosted run
  # still going is not dispatched: a newly stuck context joins that run,
  # and a retry waits for it, keeping its attempts.
  local pr branch head ctx state desc at seen reason result job url note attempt checks age ahead
  local -A want=() active=() dispatched=() branch_of=() head_of=()
  local -a claims=() aheads=()
  local sep=$'\x1f' i=0
  # A read loop, not mapfile: macOS's /bin/bash is 3.2.
  while IFS= read -r ahead; do aheads+=("$ahead"); done < <(ahead_counts "$lines")
  while IFS=$'\x1f' read -r pr branch head ctx state desc at seen; do
    ahead=${aheads[i]:-0}
    i=$((i + 1))
    branch_of[$pr]=$branch head_of[$pr]=$head
    if [ "$state" = PENDING ] && [[ $desc == "$prefix"* ]] && ! untested_job "$ctx" >/dev/null; then
      attempt=$(attempt_of "$desc")
      age=$(((t - at) / 60))
      checks=$(fetch_check_runs "$head") || {
        log "#$pr: could not read check runs"
        active[$pr]=1
        continue
      }
      IFS=$'\t' read -r result job url <<<"$(hosted_result "$ctx" <<<"$checks")"
      case "$result" in
        success)
          note=''
          [ "$ctx" != local/macos ] || note="; just mobile not run"
          # For success, the second field is the test job's URL.
          post_over "$head" "$ctx" "$state" "$desc" success \
            "$prefix: clippy and test ($(hosted_runner "$ctx")) passed$note" "$job" &&
            log "#$pr $ctx: hosted success"
          ;;
        failure)
          post_over "$head" "$ctx" "$state" "$desc" failure "$prefix: $job failed" "$url" &&
            log "#$pr $ctx: hosted failure ($job)"
          ;;
        running) active[$pr]=1 ;;
        queued | missing)
          if [ "$result" = queued ] && [ "$age" -lt "$queued_retry_min" ]; then
            active[$pr]=1
          elif [ "$result" = missing ] && [ "$age" -lt "$retry_min" ]; then
            active[$pr]=1
          elif [ "$attempt" -ge "$max_attempts" ]; then
            post_over "$head" "$ctx" "$state" "$desc" error \
              "$prefix: no hosted result after $attempt dispatches; dispatch ci.yml by hand" "$(runs_url "$branch")" &&
              log "#$pr $ctx: gave up after $attempt attempts"
          else
            want[$pr]=1
            claims+=("$pr$sep$ctx${sep}retry$sep$((attempt + 1))${sep}hosted run $result after $age min$sep$state$sep$desc")
          fi
          ;;
      esac
      continue
    fi
    reason=$(stuck_reason "$state" "$desc" "$at" "$seen" "$t" "$ahead") || continue
    # TEMPORARY (ADR 0091 item 5): no hosted mobile job to dispatch yet.
    if job=$(untested_job "$ctx"); then
      post_over "$head" "$ctx" "$state" "$desc" success \
        "$prefix: NOT tested; no hosted $job job yet (ADR 0091 item 5); $reason" "$adr_url" &&
        log "#$pr $ctx: posted untested success ($reason)"
      continue
    fi
    want[$pr]=1
    claims+=("$pr$sep$ctx${sep}new${sep}1$sep$reason$sep$state$sep$desc")
  done <<<"$lines"

  local claim n c kind a why old_state old_desc
  for n in "${!want[@]}"; do
    [ -z "${active[$n]:-}" ] || continue
    if dispatch "${branch_of[$n]}"; then
      dispatched[$n]=1
      log "#$n: dispatched ci.yml on ${branch_of[$n]}"
    else
      log "#$n: could not dispatch ci.yml on ${branch_of[$n]}"
    fi
  done
  for claim in "${claims[@]}"; do
    IFS=$'\x1f' read -r n c kind a why old_state old_desc <<<"$claim"
    if [ -n "${dispatched[$n]:-}" ]; then
      :
    elif [ -n "${active[$n]:-}" ] && [ "$kind" = new ]; then
      why="$why; joins the running hosted run"
    else
      continue
    fi
    post_over "${head_of[$n]}" "$c" "$old_state" "$old_desc" pending \
      "$prefix: hosted CI on ${head_of[$n]:0:7} (attempt $a); $why" "$(runs_url "${branch_of[$n]}")" &&
      log "#$n $c: claimed (attempt $a): $why"
  done
  return 0
}

if [ -z "${FALLBACK_SOURCED:-}" ]; then
  command -v jq >/dev/null || die "jq not found"
  tick
fi
