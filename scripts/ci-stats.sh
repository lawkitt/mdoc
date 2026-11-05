#!/usr/bin/env bash
# Usage: scripts/ci-stats.sh [--days N | --since <UTC time>]
#
# ADR 0100's CI targets, measured from GitHub over the last N days (default
# 3) or since a time such as 2026-11-05T22:20:00Z (to measure only what ran
# after a change landed), each with its median and p90 and whether it holds:
#
#   queue   a queue commit's time from its Queue run starting to its first
#           green merge-gate status: median 13 minutes or less;
#   fast    a PR head's time from its CI run starting to `fast (ubuntu)`
#           finishing green: median 4 minutes or less;
#   infra   queue commits whose merge-gate failed as infrastructure (a job
#           that never got a runner, ADR 0100): at most 1 a week.
#
# Prints one line per target and exits 1 when one fails, 0 when all hold
# (a target with no samples holds). GH_REPO picks the repository (default
# lawkitt/mdoc); `gh` must be signed in. Read-only. Bash 3.2 compatible;
# scripts/test-ci-stats.sh checks it against fakes.
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

repo=${GH_REPO:-lawkitt/mdoc}

# --- GitHub (replaced by fakes in scripts/test-ci-stats.sh) -------------------

# A workflow's runs created since $2 for event $3: `<id> <head_sha> <created>`
# lines, the time in epoch seconds.
fetch_runs() { # workflow since event
  gh api --paginate "repos/$repo/actions/workflows/$1/runs?per_page=100&event=$3&created=>=$2" \
    --jq '.workflow_runs[] | "\(.id) \(.head_sha) \(.created_at | fromdateiso8601)"'
}

# A commit's merge-gate statuses, oldest first: `<state> <epoch> <description>`.
fetch_gate() { # sha
  gh api --paginate "repos/$repo/commits/$1/statuses?per_page=100" \
    --jq '.[] | select(.context == "merge-gate") | "\(.state) \(.created_at | fromdateiso8601) \(.description)"' |
    sort -k2,2n
}

# When a run's `fast (ubuntu)` job finished green, in epoch seconds; nothing
# when it did not.
fetch_fast_done() { # run id
  gh api --paginate "repos/$repo/actions/runs/$1/jobs?per_page=100" \
    --jq '.jobs[] | select(.name == "fast (ubuntu)" and .conclusion == "success") | .completed_at | fromdateiso8601'
}

# --- decisions (pure) ----------------------------------------------------------

# `<n> <median> <p90>` of numbers on stdin (minutes, one decimal); `0 - -`
# for none.
summary() {
  sort -n | awk '{ v[NR] = $1 } END {
    if (NR == 0) { print "0 - -"; exit }
    m = (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2
    # Nearest rank: the smallest value with 90% of the samples at or below it.
    r = int(0.9 * NR); if (r < 0.9 * NR) r++; if (r < 1) r = 1
    p = v[r]
    printf "%d %.1f %.1f\n", NR, m, p
  }'
}

# Whether a median meets a limit: 0 yes (or no samples), 1 no.
within() { # median limit
  [ "$1" = - ] && return 0
  awk -v m="$1" -v l="$2" 'BEGIN { exit !(m <= l) }'
}

# A queue commit's gate lines (stdin) and its Queue run's start: the minutes
# to its first success, or nothing; and whether it failed as
# infrastructure (the wording merge-gate uses: "no <OS> runner").
queue_minutes() { # start
  awk -v s="$1" '$1 == "success" { printf "%.1f\n", ($2 - s) / 60; exit }'
}
infra_failure() {
  grep -qE '^failure [0-9]+ .*no [A-Za-z]+ runner'
}

# --- the report --------------------------------------------------------------

report() { # days [since]
  local days=$1 since=${2:-} now runs id sha start gate finished qmins='' fmins='' infra=0 rc=0
  now=$(date -u +%s)
  [ -n "$since" ] || since=$(date -u -r $((now - days * 86400)) +%Y-%m-%dT%H:%M:%SZ 2>/dev/null ||
    date -u -d "@$((now - days * 86400))" +%Y-%m-%dT%H:%M:%SZ)
  runs=$(fetch_runs queue.yml "$since" merge_group) || die "could not list the Queue runs"
  while read -r id sha start; do
    [ -n "$sha" ] || continue
    gate=$(fetch_gate "$sha") || die "could not read merge-gate on $sha"
    qmins+=$(queue_minutes "$start" <<<"$gate")$'\n'
    if infra_failure <<<"$gate"; then infra=$((infra + 1)); fi
  done < <(sort -u -k2,2 <<<"$runs")
  runs=$(fetch_runs ci.yml "$since" pull_request) || die "could not list the CI runs"
  while read -r id sha start; do
    [ -n "$id" ] || continue
    finished=$(fetch_fast_done "$id") || die "could not read run $id's jobs"
    [ -z "$finished" ] || fmins+=$(awk -v d="$finished" -v s="$start" 'BEGIN { printf "%.1f", (d - s) / 60 }')$'\n'
  done <<<"$runs"

  local n m p verdict
  read -r n m p <<<"$(awk 'NF' <<<"$qmins" | summary)"
  verdict=pass
  within "$m" 13 || verdict=FAIL rc=1
  echo "queue  $verdict  queue commit to green gate: $n commits, median $m min, p90 $p min (target: median 13 or less)"
  read -r n m p <<<"$(awk 'NF' <<<"$fmins" | summary)"
  verdict=pass
  within "$m" 4 || verdict=FAIL rc=1
  echo "fast   $verdict  PR head to fast tier: $n runs, median $m min, p90 $p min (target: median 4 or less)"
  local per_week
  per_week=$(awk -v i="$infra" -v d="$days" 'BEGIN { printf "%.1f", i * 7 / d }')
  verdict=pass
  awk -v w="$per_week" 'BEGIN { exit !(w <= 1) }' || verdict=FAIL rc=1
  echo "infra  $verdict  infrastructure failures: $infra in $days days, $per_week a week (target: 1 or fewer)"
  return "$rc"
}

if [ -z "${CI_STATS_SOURCED:-}" ]; then
  days=3
  since=''
  usage="usage: scripts/ci-stats.sh [--days N | --since <UTC time>]"
  case "${1:-}" in
    '') ;;
    --days)
      [[ ${2:-} =~ ^[1-9][0-9]*$ ]] || die "$usage"
      days=$2
      ;;
    --since)
      [[ ${2:-} =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:]{8}Z$ ]] || die "$usage"
      since=$2
      # The infrastructure rate is per week of the window, at least a day.
      days=$(jq -rn --arg s "$since" '((now - ($s | fromdateiso8601)) / 86400 | ceil) | if . < 1 then 1 else . end')
      ;;
    *) die "$usage" ;;
  esac
  command -v jq >/dev/null || die "jq not found"
  report "$days" "$since"
fi
