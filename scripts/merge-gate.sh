#!/usr/bin/env bash
# Usage: scripts/merge-gate.sh
#
# The `merge-gate` status (ADR 0095): the one required check beside `lint
# (ubuntu)`, posted on two kinds of commit.
#
#   A PR head: green when the fast tier (`fast (ubuntu)`) passed, and says
#   that the full suite runs in the merge queue; "Docs only" when the PR
#   touches no area (scripts/ci-areas.txt).
#   A queue commit (a branch under gh-readonly-queue/main/, or ci-shadow/main/
#   for the shadow trial): green only when every job its areas need passed,
#   each named with where it ran. Hosted jobs (queue.yml) are check runs
#   named ci/macos, ci/ios and ci/detection (<shard>); the PC posts commit
#   statuses ci/windows and ci/android, and refreshes a pending one at least
#   every 5 minutes while it runs (its heartbeat).
#
# Fallback: a queue commit's Windows or Android job runs hosted (ci-desktop.yml
# with os=windows, ci-android.yml) when the PC has posted nothing 10 minutes
# after the commit appeared, when its pending status is 10 minutes old (a
# stale heartbeat), or at once when the group holds a fork or bot PR, which
# never runs on the PC. Whichever result comes first counts.
#
# .github/workflows/merge-gate.yml runs main's copy of this script every 5
# minutes and after statuses and CI runs, with statuses: write and actions:
# write; it never checks out PR code. Bash 3.2 compatible (the suite runs
# under macOS /bin/bash).
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

repo=${MERGE_GATE_REPO:-${GITHUB_REPOSITORY:-lawkitt/mdoc}}
context=merge-gate
prefixes=${MERGE_GATE_PREFIXES:-gh-readonly-queue/main/ ci-shadow/main/}
silent_min=${MERGE_GATE_SILENT_MIN:-10}
run_url=${GITHUB_SERVER_URL:-https://github.com}/$repo/actions/runs/${GITHUB_RUN_ID:-0}

log() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >&2; }

# jq for Windows ends lines with CRLF (the suite may run on the PC).
jq() { command jq "$@" | tr -d '\r'; }

# --- GitHub (replaced by fakes in scripts/test-merge-gate.sh) ----------------

# shellcheck disable=SC2016 # GraphQL variables, not shell ones
heads_query='query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 50) {
      nodes {
        number isDraft headRefOid
        files(first: 100) { totalCount nodes { path } }
      }
    }
  }
}'

fetch_heads() { gh api graphql -f owner="${repo%/*}" -f name="${repo#*/}" -f query="$heads_query"; }

# The queue commits: `<branch> <sha>` lines.
fetch_queue() {
  local p
  for p in $prefixes; do
    gh api "repos/$repo/git/matching-refs/heads/$p" \
      --jq '.[] | "\(.ref | sub("^refs/heads/"; "")) \(.object.sha)"'
  done
}

fetch_main() { gh api "repos/$repo/commits/main" --jq .sha; }

# What a queue commit brings onto main: changed files and commit subjects.
fetch_compare() { gh api "repos/$repo/compare/$1...$2"; }

# When a commit was made, in epoch seconds.
fetch_commit_time() { gh api "repos/$repo/commits/$1" --jq '.commit.committer.date | fromdateiso8601'; }

fetch_check_runs() { gh api "repos/$repo/commits/$1/check-runs?per_page=100"; }

fetch_status() { gh api "repos/$repo/commits/$1/status?per_page=100"; }

# `<author type> <cross-repository>` of PR $1.
fetch_pr() { gh api "repos/$repo/pulls/$1" --jq '"\(.user.type) \(.head.repo.full_name != .base.repo.full_name)"'; }

# How many manual runs of a workflow exist for a commit.
fetch_dispatched() { # workflow sha
  gh api "repos/$repo/actions/workflows/$1/runs?event=workflow_dispatch&head_sha=$2&per_page=1" --jq .total_count
}

dispatch() { # workflow branch sha [key=value...]
  local wf=$1 branch=$2 sha=$3 args=()
  shift 3
  local kv
  for kv in "$@"; do args+=(-f "$kv"); done
  gh workflow run "$wf" --repo "$repo" --ref "$branch" -f "ref=$sha" "${args[@]}"
}

post() { # sha state description url
  gh api -X POST "repos/$repo/statuses/$1" -f "context=$context" -f "state=$2" \
    -f "description=${3:0:140}" -f "target_url=$4" >/dev/null
}

now() { date -u +%s; }

# --- decisions (pure: JSON or text in, one line out) ----------------------------

# The areas of a file list (one path per line on stdin), by main's table.
areas_of() {
  tr '\n' '\0' | changed_areas "$(cat scripts/ci-areas.txt)"
}

# A check run's verdict from check-runs JSON on stdin: the newest run of
# each job named $1, alone or after a caller's "<job> / " (a called
# workflow's job), or, when $1 ends in "(", every shard "$1<n>)". Prints
# success <count>, failure <url>, skipped, pending or missing.
check_verdict() { # name
  jq -r --arg n "$1" '
    def base: .name | sub("^.* / "; "");
    def wanted: if ($n | endswith("(")) then (base | startswith($n)) else base == $n end;
    [.check_runs[] | select(wanted)] | group_by(.name) | map(max_by(.id)) as $runs
    | if ($runs | length) == 0 then "missing"
      elif any($runs[]; .status == "completed" and (.conclusion == "failure" or .conclusion == "timed_out" or .conclusion == "cancelled" or .conclusion == "action_required"))
        then ([$runs[] | select(.status == "completed" and .conclusion != "success" and .conclusion != "skipped" and .conclusion != "neutral")] | first | "failure \(.html_url)")
      elif all($runs[]; .status == "completed" and .conclusion == "skipped") then "skipped"
      elif all($runs[]; .status == "completed") then "success \([$runs[] | select(.conclusion == "success")] | length)"
      else "pending" end'
}

# A status context's latest state, its time and its URL from combined-status
# JSON on stdin: `<state> <epoch-or-0> <url>`, or `none 0 -`.
status_of() { # context
  jq -r --arg c "$1" '
    ([.statuses[]? | select(.context == $c)] | first) as $s
    | if $s == null then "none 0 -" else "\($s.state) \($s.updated_at // $s.created_at | fromdateiso8601) \($s.target_url // "-")" end'
}

# The merge-gate state for a PR head: `<state>|<description>`.
head_gate() { # areas fast-verdict
  local areas=$1 verdict=$2
  if [ -z "$areas" ]; then
    echo "success|Docs only: no job runs"
    return 0
  fi
  case "$verdict" in
    success*) echo "success|Fast tier passed (unit, scripts); the full suite runs in the merge queue" ;;
    failure*) echo "failure|Fast tier failed: fast (ubuntu)" ;;
    *) echo "pending|Waiting for the fast tier: fast (ubuntu)" ;;
  esac
}

# The jobs a queue commit needs, from its areas: macos, windows, ios,
# android, detection (ADR 0095). The fixtures alone need only the detection
# gate; it decides itself whether recognition changed.
needed_jobs() { # areas
  local a=$1 out=''
  if affects desktop "$a"; then out+=' macos windows'; fi
  if affects ios "$a"; then out+=' ios'; fi
  if affects android "$a"; then out+=' android'; fi
  if affects desktop "$a" || affects recognition "$a"; then out+=' detection'; fi
  echo "${out# }"
}

# PR numbers from commit subjects on stdin: squash merges end "(#N)",
# merge commits say "Merge pull request #N".
group_prs() {
  sed -nE 's/.*\(#([0-9]+)\)$/\1/p; s/^Merge pull request #([0-9]+).*/\1/p' | sort -u
}

# A PC job's state (windows or android): `<verdict> <where> <url>`, where the
# verdict is success, failure, pending or fallback (run it hosted now).
pc_job() { # pc-status-line hosted-verdict hosted-dispatched third-party commit-age-min now
  local state at url hv=$2 dispatched=$3 third=$4 age=$5 t=$6 hosted_url=-
  read -r state at url <<<"$1"
  case "$hv" in failure*) hosted_url=${hv#failure } ;; esac
  # The first result counts.
  if [ "$state" = success ]; then
    echo "success PC $url"
    return 0
  fi
  case "$hv" in success*)
    echo "success hosted -"
    return 0
    ;;
  esac
  if [ "$state" = failure ] || [ "$state" = error ]; then
    echo "failure PC $url"
    return 0
  fi
  case "$hv" in failure*)
    echo "failure hosted $hosted_url"
    return 0
    ;;
  esac
  if [ "$dispatched" -gt 0 ] || [ "$hv" = pending ]; then
    echo "pending hosted -"
    return 0
  fi
  if [ "$third" = true ]; then
    echo "fallback hosted third-party"
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

# The merge-gate state for a queue commit from job lines on stdin
# (`<job> <verdict> <where> <url>`): `<state>|<description>|<url>`.
queue_gate() {
  awk -v run="$1" '
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

# Posts merge-gate on $1 unless it already says the same.
settle() { # sha state description url current-status-json
  local cur
  cur=$(jq -r --arg c "$context" '([.statuses[]? | select(.context == $c)] | first) as $s | "\($s.state // "none")|\($s.description // "")"' <<<"$5")
  if [ "$cur" = "$2|${3:0:140}" ]; then return 0; fi
  post "$1" "$2" "$3" "$4" && log "${1:0:7}: $2: $3"
}

heads_pass() {
  local json draft head total files areas checks verdict line state desc status
  json=$(fetch_heads) || {
    log "could not list PRs"
    return 1
  }
  while IFS=$'\t' read -r _ draft head total files; do
    [ "$draft" = false ] || continue
    if [ "$total" -gt 100 ]; then
      areas='core desktop recognition ios android'
    else
      areas=$(tr ',' '\n' <<<"$files" | areas_of)
    fi
    checks=$(fetch_check_runs "$head") || continue
    verdict=$(check_verdict "fast (ubuntu)" <<<"$checks")
    line=$(head_gate "$areas" "$verdict")
    state=${line%%|*}
    desc=${line#*|}
    status=$(fetch_status "$head") || continue
    settle "$head" "$state" "$desc" "$run_url" "$status"
  done < <(jq -r '.data.repository.pullRequests.nodes[]
    | [.number, .isDraft, .headRefOid, .files.totalCount, ([.files.nodes[].path] | join(","))] | @tsv' <<<"$json")
}

queue_pass() {
  local t main branch sha cmp areas jobs checks status age third prs pr kind cross job hv line state desc url v where u
  t=$(now)
  main=$(fetch_main) || {
    log "could not read main"
    return 1
  }
  while read -r branch sha; do
    [ -n "$sha" ] || continue
    cmp=$(fetch_compare "$main" "$sha") || continue
    if [ "$(jq '.files | length' <<<"$cmp")" -ge 300 ]; then
      areas='core desktop recognition ios android'
    else
      areas=$(jq -r '.files[].filename' <<<"$cmp" | areas_of)
    fi
    jobs=$(needed_jobs "$areas")
    checks=$(fetch_check_runs "$sha") || continue
    status=$(fetch_status "$sha") || continue
    age=$(((t - $(fetch_commit_time "$sha")) / 60))
    third=false
    prs=$(jq -r '.commits[].commit.message | split("\n")[0]' <<<"$cmp" | group_prs)
    [ -n "$prs" ] || third=true
    for pr in $prs; do
      read -r kind cross <<<"$(fetch_pr "$pr" || echo unknown true)"
      if [ "$kind" != User ] || [ "$cross" != false ]; then third=true; fi
    done
    local lines=''
    for job in $jobs; do
      case "$job" in
        macos | ios)
          hv=$(check_verdict "ci/$job" <<<"$checks")
          read -r v u <<<"$hv"
          case "$v" in success) where=hosted u=- ;; failure) where=hosted ;; *) where=- u=- ;; esac
          [ "$v" != missing ] || v=pending
          lines+="$job $v $where ${u:--}"$'\n'
          ;;
        detection)
          hv=$(check_verdict "ci/detection (" <<<"$checks")
          read -r v u <<<"$hv"
          case "$v" in
            success) where="${u}_shards" u=- ;;
            failure) where=hosted ;;
            *) where=- u=- ;;
          esac
          [ "$v" != missing ] || v=pending
          lines+="$job $v $where ${u:--}"$'\n'
          ;;
        windows | android)
          local wf=ci-desktop.yml extra=os=windows
          [ "$job" = windows ] || wf=ci-android.yml extra=
          line=$(pc_job "$(status_of "ci/$job" <<<"$status")" "$(check_verdict "ci/$job (hosted)" <<<"$checks")" \
            "$(fetch_dispatched "$wf" "$sha" 2>/dev/null || echo 0)" "$third" "$age" "$t")
          read -r v where u <<<"$line"
          if [ "$v" = fallback ]; then
            # shellcheck disable=SC2086 # extra is one key=value or nothing
            if dispatch "$wf" "$branch" "$sha" $extra; then
              log "${sha:0:7} $job: hosted fallback dispatched ($u)"
            else
              log "${sha:0:7} $job: could not dispatch $wf"
            fi
            v=pending u=-
          fi
          lines+="$job $v $where ${u:--}"$'\n'
          ;;
      esac
    done
    line=$(printf '%s' "$lines" | queue_gate "$run_url")
    state=${line%%|*}
    desc=${line#*|}
    url=${desc##*|}
    desc=${desc%|*}
    settle "$sha" "$state" "$desc" "$url" "$status"
  done < <(fetch_queue)
}

if [ -z "${MERGE_GATE_SOURCED:-}" ]; then
  command -v jq >/dev/null || die "jq not found"
  rc=0
  heads_pass || rc=1
  queue_pass || rc=1
  exit "$rc"
fi
