#!/usr/bin/env bash
# Usage: scripts/ci-stats.sh [--days N | --since <UTC time>] [--nights K]
#
# ADR 0112's CI targets, measured from GitHub's queue commits (the merge
# queue's Queue runs) over the last N days (default 7) or since a time such
# as 2026-11-05T22:20:00Z (to measure only what ran after a change landed):
#
#   queue    a queue commit's time from its Queue run starting to its first
#            green merge-gate status: median 25 minutes or less;
#   windows  a green ci/windows run's time: median 15 minutes or less. The
#            PC's comes from its status ("Passed in 13m45s"); a hosted
#            fallback's (ci-desktop.yml, dispatched by merge-gate for a
#            commit whose gate names "windows (hosted)") is its jobs' time,
#            from the first `ci/windows (hosted, ...)` job starting to the
#            last finishing, all green;
#   ios-ui   a green `ci/ios (ui)` job's time on hosted macOS: median 20
#            minutes or less;
#   drops    queue drops not caused by the PR: at most 1 a week.
#
# Each line gives the samples' median and p90; failed and timed-out runs give
# no time. A drop is a queue commit whose merge-gate failed (the queue then
# drops its PR). It counts as not caused by the PR when every job its first
# failing merge-gate status names is one of:
#   - an infrastructure failure (ADR 0100), as merge-gate words it: "no macOS
#     runner, 3 attempts" or "did not run";
#   - a PC job (ci/windows, ci/android) whose failing status names a test
#     (the first it names) that is on the quarantine list
#     (.config/quarantine.txt: an app UI line by name, a Rust line whose
#     nextest filter contains the test's path), or that the PC named as
#     failing on another PR's queue commit within 14 days either side,
#     among the queue commits this run reads.
# Anything else counts as caused by the PR: a hosted job's failure (it names
# no test), a PC timeout or a failure naming no test. Failures on main or in
# the nightly, and a failing test outside the PR's areas, are not read.
# Unrelated drops are listed under the drops line, with the rule that matched.
#
# --nights K measures K windows, ending now, a day ago and so on (each N days
# long, from the same reads), as the nightly saw them on its last K nights.
#
# Exits 0 when every target holds (a target with no samples holds), 2 when
# one fails in the latest window, 3 when one fails in every one of K >= 2
# windows, 1 on an error. The limits can be overridden for a drill:
# CI_STATS_QUEUE_MIN, CI_STATS_WINDOWS_MIN, CI_STATS_IOS_UI_MIN (minutes),
# CI_STATS_DROPS_WEEK. GH_REPO picks the repository (default lawkitt/mdoc);
# `gh` must be signed in. Read-only. Bash 3.2 compatible;
# scripts/test-ci-stats.sh checks it against fakes.
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

repo=${GH_REPO:-lawkitt/mdoc}
quarantine=${CI_STATS_QUARANTINE:-$(dirname "${BASH_SOURCE[0]}")/../.config/quarantine.txt}
queue_min=${CI_STATS_QUEUE_MIN:-25}
windows_min=${CI_STATS_WINDOWS_MIN:-15}
ios_ui_min=${CI_STATS_IOS_UI_MIN:-20}
drops_week=${CI_STATS_DROPS_WEEK:-1}
# The repeat rule's reach either side of a drop.
repeat_s=$((14 * 86400))

# --- GitHub (replaced by fakes in scripts/test-ci-stats.sh) -------------------

# `gh api` with 3 tries, 10 and 30 seconds apart: a nightly read of a few
# hundred calls meets GitHub's occasional 502.
gh_api() {
  local try out
  for try in 1 2 3; do
    if out=$(gh api "$@"); then
      printf '%s\n' "$out" | awk 'NF'
      return 0
    fi
    [ "$try" -eq 3 ] || sleep $((try == 1 ? 10 : 30))
  done
  return 1
}

# A workflow's runs created from $2 to $3 (UTC times, both inclusive) for
# event $4: `<id> <head_sha> <created> <head_branch>` lines, the time in
# epoch seconds. GitHub answers a filtered run list with at most 1,000 runs,
# so the caller asks a day at a time.
fetch_runs() { # workflow from to event
  gh_api --paginate "repos/$repo/actions/workflows/$1/runs?per_page=100&event=$4&created=$2..$3" \
    --jq '.workflow_runs[] | "\(.id) \(.head_sha) \(.created_at | fromdateiso8601) \(.head_branch)"'
}

# A commit's statuses, every context: `<context> <state> <epoch> <description>`.
fetch_statuses() { # sha
  gh_api --paginate "repos/$repo/commits/$1/statuses?per_page=100" \
    --jq '.[] | "\(.context) \(.state) \(.created_at | fromdateiso8601) \(.description // "")"'
}

# A run's jobs (its latest attempt): `<conclusion> <started> <completed> <name>`,
# times in epoch seconds (0 when unset).
fetch_jobs() { # run id
  gh_api --paginate "repos/$repo/actions/runs/$1/jobs?per_page=100" \
    --jq '.jobs[] | "\(.conclusion // "none") \(.started_at // "" | if . == "" then 0 else fromdateiso8601 end) \(.completed_at // "" | if . == "" then 0 else fromdateiso8601 end) \(.name)"'
}

# The ids of a workflow's dispatched runs for a commit (merge-gate's hosted
# fallbacks).
fetch_dispatched() { # workflow sha
  gh_api --paginate "repos/$repo/actions/workflows/$1/runs?per_page=100&event=workflow_dispatch&head_sha=$2" \
    --jq '.workflow_runs[].id'
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
# merge-gate (its merge-gate lines, `<state> <epoch> <description>` oldest
# first, on stdin), or nothing.
queue_seconds() { # start
  awk -v s="$1" '$1 == "success" { print $2 - s; exit }'
}

# The description of a commit's first failing merge-gate status (the same
# lines on stdin), or nothing: the queue dropped it.
gate_failure() {
  awk '$1 == "failure" { $1 = ""; $2 = ""; sub(/^  /, ""); print; exit }'
}

# The jobs a failing merge-gate description names, `<job> <where>` lines:
# "Failed: windows (PC), ios (no macOS runner, 3 attempts); passed: ..."
# gives `windows PC` and `ios no macOS runner, 3 attempts`.
failed_jobs() { # description
  local failed=${1#Failed: }
  [ "$failed" != "$1" ] || return 0
  failed=${failed%%; passed:*}
  grep -oE '[a-z0-9_-]+ \([^)]*\)' <<<"$failed" | sed -E 's/^([^ ]+) \((.*)\)$/\1 \2/'
}

# Whether a job's place (from failed_jobs) is merge-gate's wording for an
# infrastructure failure (ADR 0100).
infra_where() { # where
  [[ $1 =~ ^no\ [A-Za-z]+\ runner || $1 =~ ^did\ not\ run ]]
}

# The test a PC failure status names ("Failed in just test: ( 714/1634)
# mdoc-markdown from_html::tests::x and 2 more" gives `mdoc-markdown
# from_html::tests::x`), or nothing.
pc_test() { # description
  local d=$1
  case "$d" in "Failed in "*": "*) ;; *) return 0 ;; esac
  d=${d#*: }
  sed -E 's/ and [0-9]+ more$//; s/^\( *[0-9]+\/[0-9]+\) //' <<<"$d"
}

# Seconds from the PC's "Passed in 13m45s", or nothing.
pc_seconds() { # description
  [[ $1 =~ ^Passed\ in\ ([0-9]+)m([0-9]+)s$ ]] || return 0
  echo $((10#${BASH_REMATCH[1]} * 60 + 10#${BASH_REMATCH[2]}))
}

# A hosted Windows fallback's seconds from its jobs (fetch_jobs lines on
# stdin): the first `ci/windows (hosted, ...)` job's start to the last one's
# end, when all of them passed; otherwise nothing.
hosted_windows_seconds() {
  awk '{ name = $0; sub(/^[^ ]+ [^ ]+ [^ ]+ /, "", name) }
    index(name, "ci/windows (hosted, ") == 1 {
      n++; if ($1 != "success" || $2 == 0 || $3 == 0) bad = 1
      if (s == "" || $2 < s) s = $2; if ($3 > e) e = $3
    }
    END { if (n && !bad) print e - s }'
}

# A green `ci/ios (ui)` job's seconds from a Queue run's jobs on stdin (named
# `ios / ci/ios (ui)` there), or nothing.
ios_ui_seconds() {
  awk '{ name = $0; sub(/^[^ ]+ [^ ]+ [^ ]+ /, "", name) }
    name ~ /(^|\/ )ci\/ios \(ui\)$/ && $1 == "success" && $2 > 0 && $3 > 0 { print $3 - $2; exit }'
}

# Whether a test is on the quarantine list: an app UI line (android:, ios:)
# by its name, a Rust line when its nextest filter contains the test's path
# (the last word of `<binary> <path>`) as a whole name.
quarantined() { # test
  [ -f "$quarantine" ] || return 1
  awk -v t="$1" '
    BEGIN { app = (t ~ /^(android|ios):/); p = t; sub(/.* /, "", p) }
    /^[ \t]*(#|$)/ { next }
    {
      f = $0; sub(/  +#.*/, "", f); sub(/[ \t]+$/, "", f)
      if (app) { if (f == t) found = 1; next }
      if (f ~ /^(android|ios):/) next
      s = f
      while ((i = index(s, p)) > 0) {
        c = substr(s, i + length(p), 1)
        b = (i > 1) ? substr(s, i - 1, 1) : ""
        if (c !~ /[A-Za-z0-9_]/ && b !~ /[A-Za-z0-9_]/) { found = 1; break }
        s = substr(s, i + 1)
      }
    }
    END { exit !found }' "$quarantine"
}

# Why a drop was not caused by its PR, or `related`: every job its failing
# merge-gate description names is infrastructure, or a PC job whose failing
# test is quarantined or failed on another PR's queue commit within 14 days
# (tests file lines: `<created> <sha> <pr> <job> <test>`). The first job's
# rule names the drop.
drop_reason() { # sha pr created description tests-file
  local sha=$1 pr=$2 at=$3 desc=$4 tests=$5 job where test why='' rule n=0
  while read -r job where; do
    [ -n "$job" ] || continue
    n=$((n + 1))
    rule=''
    if infra_where "$where"; then
      rule="infrastructure ($job $where)"
    elif [ "$where" = PC ] && { [ "$job" = windows ] || [ "$job" = android ]; }; then
      test=$(awk -v s="$sha" -v j="$job" '$2 == s && $4 == j { $1 = $2 = $3 = $4 = ""; sub(/^ +/, ""); print; exit }' "$tests")
      if [ -n "$test" ] && quarantined "$test"; then
        rule="quarantined ($test)"
      elif [ -n "$test" ] && awk -v p="$pr" -v a="$at" -v r="$repeat_s" -v j="$job" -v t="$test" '
        { x = $0; sub(/^[^ ]+ [^ ]+ [^ ]+ [^ ]+ /, "", x) }
        $3 != p && $4 == j && x == t && ($1 - a <= r) && (a - $1 <= r) { found = 1; exit }
        END { exit !found }' "$tests"; then
        rule="repeated ($test, on another PR's queue commit)"
      fi
    fi
    [ -n "$rule" ] || {
      echo related
      return 0
    }
    [ -n "$why" ] || why=$rule
  done < <(failed_jobs "$desc")
  if [ "$n" -eq 0 ]; then echo related; else echo "$why"; fi
}

# A queue branch's PR number (gh-readonly-queue/main/pr-823-<sha>), or `-`.
branch_pr() { # branch
  [[ $1 =~ /pr-([0-9]+)- ]] && echo "${BASH_REMATCH[1]}" || echo -
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

# --- reading GitHub once ---------------------------------------------------------

# Reads the queue commits whose first Queue run started from $1 to $2 into
# files in $3, one sample a line, `<created>` first: queue (seconds to green),
# windows (`<seconds> PC|hosted`), ios-ui (seconds), drops (`<sha> <pr>
# <description>`) and tests (`<sha> <pr> <job> <test>`, every PC failure
# naming one).
gather() { # from to dir
  local dir=$3 runs id sha at branch pr st gate desc line ctx state t rest secs jobs rid rids
  : >"$dir/queue" && : >"$dir/windows" && : >"$dir/ios-ui" && : >"$dir/drops" && : >"$dir/tests"
  runs=$(runs_between queue.yml "$1" "$2" merge_group) || die "could not list the Queue runs"
  # Each commit once, with its first run.
  while read -r id sha at branch; do
    [ -n "$sha" ] || continue
    pr=$(branch_pr "$branch")
    st=$(fetch_statuses "$sha") || die "could not read the statuses of $sha"
    st=$(sort -s -k3,3n <<<"$st")
    gate=$(awk '$1 == "merge-gate" { $1 = ""; sub(/^ /, ""); print }' <<<"$st")
    secs=$(queue_seconds "$at" <<<"$gate")
    [ -z "$secs" ] || echo "$at $secs" >>"$dir/queue"
    desc=$(gate_failure <<<"$gate")
    [ -z "$desc" ] || echo "$at $sha $pr $desc" >>"$dir/drops"
    while read -r ctx state t rest; do
      case "$ctx $state" in
        "ci/windows success")
          secs=$(pc_seconds "$rest")
          [ -z "$secs" ] || echo "$at $secs PC" >>"$dir/windows"
          ;;
        "ci/windows failure" | "ci/android failure")
          line=$(pc_test "$rest")
          [ -z "$line" ] || echo "$at $sha $pr ${ctx#ci/} $line" >>"$dir/tests"
          ;;
      esac
    done <<<"$st"
    if grep -q 'windows (hosted)' <<<"$gate"; then
      rids=$(fetch_dispatched ci-desktop.yml "$sha") || die "could not list the hosted Windows runs of $sha"
      for rid in $rids; do
        jobs=$(fetch_jobs "$rid") || die "could not read run $rid's jobs"
        secs=$(hosted_windows_seconds <<<"$jobs")
        [ -z "$secs" ] || echo "$at $secs hosted" >>"$dir/windows"
      done
    fi
    if grep -q 'ios' <<<"$gate"; then
      jobs=$(fetch_jobs "$id") || die "could not read run $id's jobs"
      secs=$(ios_ui_seconds <<<"$jobs")
      [ -z "$secs" ] || echo "$at $secs" >>"$dir/ios-ui"
    fi
  done < <(sort -k3,3n <<<"$runs" | awk '!seen[$2]++')
}

# --- the report --------------------------------------------------------------

# The second field of a sample file's lines whose time is from $2 to $3.
samples() { # file from to
  awk -v f="$2" -v t="$3" '$1 >= f && $1 <= t { print $2 }' "$1"
}

# One line per target over the samples from $1 to $2 (the files gather
# wrote in $3): 0 when every target holds, 1 when one fails.
report() { # from to dir
  local from=$1 to=$2 dir=$3 rc=0 n m p verdict pc hosted days per_week
  local at sha pr desc why unrelated=0 drops=0 infra=0 quar=0 rep=0 list=''
  read -r n m p <<<"$(samples "$dir/queue" "$from" "$to" | summary)"
  verdict=pass
  within "$m" "$queue_min" || verdict=FAIL rc=1
  echo "queue    $verdict  queue commit to green gate: $n commits, median $(minutes "$m") min, p90 $(minutes "$p") min (target: median $queue_min or less)"
  read -r n m p <<<"$(samples "$dir/windows" "$from" "$to" | summary)"
  pc=$(awk -v f="$from" -v t="$to" '$1 >= f && $1 <= t && $3 == "PC"' "$dir/windows" | wc -l | tr -d ' ')
  hosted=$((n - pc))
  verdict=pass
  within "$m" "$windows_min" || verdict=FAIL rc=1
  echo "windows  $verdict  green ci/windows runs: $n ($pc on the PC, $hosted hosted), median $(minutes "$m") min, p90 $(minutes "$p") min (target: median $windows_min or less)"
  read -r n m p <<<"$(samples "$dir/ios-ui" "$from" "$to" | summary)"
  verdict=pass
  within "$m" "$ios_ui_min" || verdict=FAIL rc=1
  echo "ios-ui   $verdict  green ci/ios (ui) jobs: $n, median $(minutes "$m") min, p90 $(minutes "$p") min (target: median $ios_ui_min or less)"
  while read -r at sha pr desc; do
    [ -n "$sha" ] || continue
    drops=$((drops + 1))
    why=$(drop_reason "$sha" "$pr" "$at" "$desc" "$dir/tests")
    case "$why" in
      related) continue ;;
      infrastructure*) infra=$((infra + 1)) ;;
      quarantined*) quar=$((quar + 1)) ;;
      *) rep=$((rep + 1)) ;;
    esac
    unrelated=$((unrelated + 1))
    list+="         $(iso "$at") ${sha:0:7} PR #$pr: $why"$'\n'
  done < <(awk -v f="$from" -v t="$to" '$1 >= f && $1 <= t' "$dir/drops")
  # The rate over the window's real length, compared unrounded.
  days=$(awk -v w=$((to - from)) 'BEGIN { printf "%.1f", w / 86400 }')
  per_week=$(awk -v i="$unrelated" -v w=$((to - from)) 'BEGIN { printf "%.2f", i * 7 * 86400 / w }')
  verdict=pass
  awk -v i="$unrelated" -v w=$((to - from)) -v l="$drops_week" 'BEGIN { exit !(i * 7 * 86400 <= l * w) }' || verdict=FAIL rc=1
  echo "drops    $verdict  queue drops not caused by the PR: $unrelated of $drops drops in $days days ($infra infrastructure, $quar quarantined test, $rep repeated test), $per_week a week (target: $drops_week or fewer)"
  printf '%s' "$list"
  return "$rc"
}

# Reads once, then reports on K windows of N days ending now, a day before
# and so on: 0 all hold in the latest, 2 one fails in it, 3 one fails in
# each of K >= 2.
measure() { # from now nights
  local from=$1 now=$2 k=$3 len=$(($2 - $1)) dir i to red=0 latest=0
  dir=$(mktemp -d) || die "no temporary directory"
  # shellcheck disable=SC2064
  trap "rm -rf '$dir'" EXIT
  gather "$((from - (k - 1) * 86400))" "$now" "$dir"
  i=0
  while [ "$i" -lt "$k" ]; do
    to=$((now - i * 86400))
    [ "$k" -eq 1 ] || echo "== $(iso $((to - len))) to $(iso "$to") =="
    if ! report $((to - len)) "$to" "$dir"; then
      red=$((red + 1))
      [ "$i" -ne 0 ] || latest=1
    fi
    i=$((i + 1))
  done
  [ "$k" -eq 1 ] || echo "missed a target in $red of $k windows"
  if [ "$k" -gt 1 ] && [ "$red" -eq "$k" ]; then return 3; fi
  [ "$latest" -eq 0 ] || return 2
  return 0
}

if [ -z "${CI_STATS_SOURCED:-}" ]; then
  usage="usage: scripts/ci-stats.sh [--days N | --since <UTC time>] [--nights K]"
  command -v jq >/dev/null || die "jq not found"
  now=$(date -u +%s)
  from=$((now - 7 * 86400))
  nights=1 since=''
  while [ $# -gt 0 ]; do
    case "$1" in
      --days)
        [[ ${2:-} =~ ^[1-9][0-9]*$ ]] || die "$usage"
        from=$((now - $2 * 86400))
        ;;
      --since)
        [[ ${2:-} =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:]{8}Z$ ]] || die "$usage"
        from=$(jq -rn --arg s "$2" '$s | fromdateiso8601') || die "$usage"
        [ "$from" -lt "$now" ] || die "--since is in the future"
        since=1
        ;;
      --nights)
        [[ ${2:-} =~ ^[1-9]$ ]] || die "$usage"
        nights=$2
        ;;
      *) die "$usage" ;;
    esac
    shift 2
  done
  [ -z "$since" ] || [ "$nights" -eq 1 ] || die "--nights takes --days, not --since"
  measure "$from" "$now" "$nights"
fi
