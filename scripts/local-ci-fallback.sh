#!/usr/bin/env bash
# Usage: scripts/local-ci-fallback.sh
#
# The hosted fallback for local CI (ADR 0082). When a PR head's local/macos
# or local/windows status is stuck, it dispatches the full hosted CI run on
# the PR branch and, when that run's clippy and test jobs for the same OS
# finish, posts their result under the stuck context, so the merge gate
# (lint (ubuntu), local/windows, local/macos) stays the same.
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
#
# The fallback's own statuses start with "Hosted fallback". A runner that
# comes back still tests such a head and its status replaces the fallback's;
# the fallback never overwrites a status it did not post. A dispatch with no
# result in 25 min (cancelled or never started) is retried, 3 attempts in all.
# Hosted CI has no `just mobile`: a local/macos it posts says so.
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

repo=${LOCAL_CI_REPO:-lawkitt/mdoc}
prefix="Hosted fallback"
contexts=(local/macos local/windows)
no_status_min=${FALLBACK_NO_STATUS_MIN:-15}
waiting_min=${FALLBACK_WAITING_MIN:-15}
running_min=${FALLBACK_RUNNING_MIN:-50}
queued_min=${FALLBACK_QUEUED_MIN:-45}
retry_min=${FALLBACK_RETRY_MIN:-25}
max_attempts=${FALLBACK_MAX_ATTEMPTS:-3}

# The hosted runner that stands in for each context: ci.yml's shipped matrix.
hosted_runner() {
  case "$1" in
    local/macos) echo macos-26 ;;
    local/windows) echo windows-latest ;;
    *) return 1 ;;
  esac
}

log() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }

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

# Why the context is stuck, or nothing (exit 1) when it is not.
stuck_reason() { # state description status-time seen-time now
  local state=$1 desc=$2 at=$3 seen=$4 t=$5 age limit
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
      [ "$age" -ge "$limit" ] || return 1
      echo "\"${desc:0:40}\" for ${age} min"
      ;;
    *) return 1 ;;
  esac
}

attempt_of() { # description
  if [[ $1 =~ \(attempt\ ([0-9]+)\) ]]; then echo "${BASH_REMATCH[1]}"; else echo 1; fi
}

# The hosted result for a context from check-runs JSON on stdin, tab
# separated: "success <url>", "failure <job> <url>", "running", or "missing"
# (a job not there, or one that ended cancelled or skipped).
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
      else "running" end'
}

runs_url() { echo "https://github.com/$repo/actions/workflows/ci.yml?query=branch%3A$1"; }

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

  # Per PR: claims and retries to dispatch, then post; one dispatch serves
  # both contexts (the full matrix), and a second one would cancel the first
  # (ci.yml's concurrency group).
  local pr branch head ctx state desc at seen reason result job url note attempt checks
  local -A want=() active=() branch_of=() head_of=()
  local -a claims=()
  while IFS=$'\x1f' read -r pr branch head ctx state desc at seen; do
    branch_of[$pr]=$branch head_of[$pr]=$head
    if [ "$state" = PENDING ] && [[ $desc == "$prefix"* ]]; then
      attempt=$(attempt_of "$desc")
      checks=$(fetch_check_runs "$head") || {
        log "#$pr: could not read check runs"
        continue
      }
      IFS=$'\t' read -r result job url <<<"$(hosted_result "$ctx" <<<"$checks")"
      case "$result" in
        success)
          note=''
          [ "$ctx" != local/macos ] || note="; just mobile not run"
          # For success, the second field is the test job's URL.
          post "$head" "$ctx" success "$prefix: clippy and test ($(hosted_runner "$ctx")) passed$note" "$job"
          log "#$pr $ctx: hosted success"
          ;;
        failure)
          post "$head" "$ctx" failure "$prefix: $job failed" "$url"
          log "#$pr $ctx: hosted failure ($job)"
          ;;
        running)
          active[$pr]=1
          ;;
        missing)
          if [ $(((t - at) / 60)) -lt "$retry_min" ]; then
            active[$pr]=1
          elif [ "$attempt" -ge "$max_attempts" ]; then
            post "$head" "$ctx" error "$prefix: no hosted result after $attempt dispatches; dispatch ci.yml by hand" "$(runs_url "$branch")"
            log "#$pr $ctx: gave up after $attempt attempts"
          else
            want[$pr]=1
            claims+=("$pr $ctx $((attempt + 1)) no hosted result in ${retry_min} min")
          fi
          ;;
      esac
      continue
    fi
    reason=$(stuck_reason "$state" "$desc" "$at" "$seen" "$t") || continue
    want[$pr]=1
    claims+=("$pr $ctx 1 $reason")
  done <<<"$lines"

  local claim n c a why
  for n in "${!want[@]}"; do
    # A run already serving this head's other context covers this one too.
    [ -n "${active[$n]:-}" ] && continue
    if dispatch "${branch_of[$n]}"; then
      log "#$n: dispatched ci.yml on ${branch_of[$n]}"
    else
      log "#$n: could not dispatch ci.yml on ${branch_of[$n]}"
      unset "want[$n]"
    fi
  done
  for claim in "${claims[@]}"; do
    read -r n c a why <<<"$claim"
    [ -n "${want[$n]:-}" ] || continue
    post "${head_of[$n]}" "$c" pending \
      "$prefix: hosted CI on ${head_of[$n]:0:7} (attempt $a); $why" "$(runs_url "${branch_of[$n]}")"
    log "#$n $c: claimed (attempt $a): $why"
  done
}

if [ -z "${FALLBACK_SOURCED:-}" ]; then
  command -v jq >/dev/null || die "jq not found"
  tick
fi
