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

# A workflow's runs created from $2 to $3 (UTC times, both inclusive) for
# event $4: `<id> <head_sha> <created>` lines, the time in epoch seconds.
# GitHub answers a filtered run list with at most 1,000 runs, so report asks
# a day at a time.
fetch_runs() { # workflow from to event
  gh api --paginate "repos/$repo/actions/workflows/$1/runs?per_page=100&event=$4&created=$2..$3" \
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

# `<n> <median> <p90>` of numbers on stdin (seconds), unrounded; `0 - -` for
# none. The p90 is the nearest rank: the smallest value with 90% of the
# samples at or below it.
summary() {
  sort -n | awk '{ v[NR] = $1 } END {
    if (NR == 0) { print "0 - -"; exit }
    m = (NR % 2) ? v[(NR + 1) / 2] : (v[NR / 2] + v[NR / 2 + 1]) / 2
    r = int(0.9 * NR); if (r < 0.9 * NR) r++; if (r < 1) r = 1
    print NR, m, v[r]
  }'
}

# Seconds as minutes for display, one decimal; `-` stays.
minutes() {
  if [ "$1" = - ]; then
    echo -
  else
    awk -v s="$1" 'BEGIN { printf "%.1f", s / 60 }'
  fi
}

# Whether a median in seconds meets a limit in minutes, compared unrounded:
# 0 yes (or no samples), 1 no.
within() { # median-seconds limit-minutes
  [ "$1" = - ] && return 0
  awk -v m="$1" -v l="$2" 'BEGIN { exit !(m <= l * 60) }'
}

# A queue commit's seconds from its Queue run's start to its first green
# merge-gate (gate lines on stdin), or nothing; and whether it failed as
# infrastructure (the wording merge-gate uses: "no <OS> runner").
queue_seconds() { # start
  awk -v s="$1" '$1 == "success" { print $2 - s; exit }'
}
infra_failure() {
  grep -qE '^failure [0-9]+ .*no [A-Za-z]+ runner'
}

iso() { date -u -r "$1" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d "@$1" +%Y-%m-%dT%H:%M:%SZ; }

# A workflow's runs from $2 to $3 (epoch seconds), a day at a time, each run
# once.
runs_between() { # workflow from to event
  local t=$2 next
  while [ "$t" -lt "$3" ]; do
    next=$((t + 86400))
    [ "$next" -le "$3" ] || next=$3
    fetch_runs "$1" "$(iso "$t")" "$(iso "$next")" "$4" || return 1
    t=$next
  done | sort -u -k1,1
}

# --- the report --------------------------------------------------------------

report() { # from to (epoch seconds)
  local from=$1 to=$2 runs id sha start gate finished qsecs='' fsecs='' infra=0 rc=0
  runs=$(runs_between queue.yml "$from" "$to" merge_group) || die "could not list the Queue runs"
  while read -r id sha start; do
    [ -n "$sha" ] || continue
    gate=$(fetch_gate "$sha") || die "could not read merge-gate on $sha"
    qsecs+=$(queue_seconds "$start" <<<"$gate")$'\n'
    if infra_failure <<<"$gate"; then infra=$((infra + 1)); fi
  done < <(sort -u -k2,2 <<<"$runs")
  runs=$(runs_between ci.yml "$from" "$to" pull_request) || die "could not list the CI runs"
  while read -r id sha start; do
    [ -n "$id" ] || continue
    finished=$(fetch_fast_done "$id") || die "could not read run $id's jobs"
    [ -z "$finished" ] || fsecs+=$((finished - start))$'\n'
  done <<<"$runs"

  local n m p verdict days per_week
  read -r n m p <<<"$(awk 'NF' <<<"$qsecs" | summary)"
  verdict=pass
  within "$m" 13 || verdict=FAIL rc=1
  echo "queue  $verdict  queue commit to green gate: $n commits, median $(minutes "$m") min, p90 $(minutes "$p") min (target: median 13 or less)"
  read -r n m p <<<"$(awk 'NF' <<<"$fsecs" | summary)"
  verdict=pass
  within "$m" 4 || verdict=FAIL rc=1
  echo "fast   $verdict  PR head to fast tier: $n runs, median $(minutes "$m") min, p90 $(minutes "$p") min (target: median 4 or less)"
  # The rate over the window's real length, compared unrounded.
  days=$(awk -v w=$((to - from)) 'BEGIN { printf "%.1f", w / 86400 }')
  per_week=$(awk -v i="$infra" -v w=$((to - from)) 'BEGIN { printf "%.2f", i * 7 * 86400 / w }')
  verdict=pass
  awk -v i="$infra" -v w=$((to - from)) 'BEGIN { exit !(i * 7 * 86400 <= w) }' || verdict=FAIL rc=1
  echo "infra  $verdict  infrastructure failures: $infra in $days days, $per_week a week (target: 1 or fewer)"
  return "$rc"
}

if [ -z "${CI_STATS_SOURCED:-}" ]; then
  usage="usage: scripts/ci-stats.sh [--days N | --since <UTC time>]"
  command -v jq >/dev/null || die "jq not found"
  now=$(date -u +%s)
  from=$((now - 3 * 86400))
  case "${1:-}" in
    '') ;;
    --days)
      [[ ${2:-} =~ ^[1-9][0-9]*$ ]] || die "$usage"
      from=$((now - $2 * 86400))
      ;;
    --since)
      [[ ${2:-} =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:]{8}Z$ ]] || die "$usage"
      from=$(jq -rn --arg s "$2" '$s | fromdateiso8601') || die "$usage"
      [ "$from" -lt "$now" ] || die "--since is in the future"
      ;;
    *) die "$usage" ;;
  esac
  report "$from" "$now"
fi
