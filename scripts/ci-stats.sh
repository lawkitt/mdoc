#!/usr/bin/env bash
# Usage: scripts/ci-stats.sh [--days N | --since <UTC time>] [--nights K]
#
# ADR 0113's CI targets, measured from GitHub's queue commits (the merge
# queue's Queue runs) over the last N days (default 7) or since a time such
# as 2026-11-05T22:20:00Z (to measure only what ran after a change landed):
#
#   core     a core queue commit's time to green: median 20 minutes or less;
#   desktop  a desktop-only one's: median 15 minutes or less;
#   queue    every queue commit's: median 25 minutes or less. A queue
#            commit's time runs from its Queue run starting to its first
#            green merge-gate status;
#   ios-ui   the slower iOS UI shard's time on hosted macOS, per queue
#            commit whose `ci/ios (ui 1)` and `(ui 2)` jobs both passed (or,
#            before the second shard, whose single `ci/ios (ui)` did): median
#            15 minutes or less;
#   drops    queue drops not caused by the PR: at most 1 a week.
#
# A queue commit's class comes from the jobs its Queue run started (not
# skipped): core when they include an iOS job (`ci/ios ...`) and a macOS or
# detection job (`ci/macos ...`, `ci/detection ...`), desktop-only with a
# macOS or detection job and no iOS one, iOS-only with an iOS job alone,
# other with none (docs, scripts, the PC's jobs). The classes line counts
# every queue commit in the window, green or not.
# Each line gives the samples' median and p90; failed and timed-out runs give
# no time. A drop is a queue commit whose merge-gate failed (the queue then
# drops its PR). It counts as not caused by the PR when every job its first
# failing merge-gate status names is one of:
#   - an infrastructure failure (ADR 0100), as merge-gate words it: "no macOS
#     runner, 3 attempts", "cancelled on macOS, 3 attempts" or "did not run";
#   - a PC job (ci/windows, ci/android) whose failing status names one test
#     (not "<test> and 2 more": the others are unknown) that is on the
#     quarantine list (.config/quarantine.txt as it is now: an app UI line by
#     name, a Rust line whose nextest filter names the test's path, and its
#     package if the filter names one), or that the PC named as failing on
#     another PR's queue commit within 14 days either side. That evidence
#     comes only from the queue commits this run reads (the window, or with
#     --nights the oldest window onwards), nothing older.
# Anything else counts as caused by the PR: a hosted job's failure (it names
# no test), a PC timeout or a failure naming no test. Failures on main or in
# the nightly, and a failing test outside the PR's areas, are not read.
# Unrelated drops are listed under the drops line, with the rule that matched.
#
# --nights K measures K windows, ending now, a day ago and so on (each N days
# long), from one read, as the nightly saw them on its last K nights: a
# window counts only what had happened by its end (a commit green by then, a
# drop and the failures that explain it posted by then). Only the quarantine
# list is today's.
#
# Exits 0 when every target holds (a target with no samples holds), 2 when
# one fails in the latest window, 3 when one fails in every one of K >= 2
# windows, 1 on an error. CI_STATS_DRILL=1 fails every window with a `drill`
# line, to drill the nightly's issue; CI_STATS_CORE_MIN, CI_STATS_DESKTOP_MIN,
# CI_STATS_QUEUE_MIN, CI_STATS_IOS_UI_MIN (minutes) and CI_STATS_DROPS_WEEK
# override the limits. Each target reads only queue commits from its start
# (target_start below); a target whose start is pending is reported, but its
# verdict never sets the exit.
# GH_REPO picks the repository (default lawkitt/mdoc); `gh` must be signed
# in. Read-only: a week's read is one statuses and one jobs read per queue
# commit (about 300 calls) and a run list a day, inside the nightly token's
# 1,000 an hour. Bash 3.2 compatible;
# scripts/test-ci-stats.sh checks it against fakes.
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

repo=${GH_REPO:-lawkitt/mdoc}
quarantine=${CI_STATS_QUARANTINE:-$(dirname "${BASH_SOURCE[0]}")/../.config/quarantine.txt}
queue_min=${CI_STATS_QUEUE_MIN:-25}
core_min=${CI_STATS_CORE_MIN:-20}
desktop_min=${CI_STATS_DESKTOP_MIN:-15}
ios_ui_min=${CI_STATS_IOS_UI_MIN:-15}
drops_week=${CI_STATS_DROPS_WEEK:-1}
drill=${CI_STATS_DRILL:-}
# The repeat rule's reach either side of a drop.
repeat_s=$((14 * 86400))

# Each target's start (UTC), the only place to set them: the merge of the
# last change the target depends on (ROADMAP's CI speed items). A target
# reads only queue commits from its start on, so the nightly's 3 red nights
# judge only data from after that change. `pending` (the change has not
# merged): the line reports every sample and its verdict but never fails
# the run; set the date when the change merges.
#   core     items 27 (one rule pack), 28 (UI shards by time) and 31
#            (detection on hosted Windows)
#   desktop  item 24 (detection only when its inputs change)
#   queue    ADR 0112's rollout, the median it keeps as a floor
#   ios-ui   item 28 (UI shards by time)
#   drops    ADR 0112's rollout
target_start() { # target
  case "$1" in
    core) echo pending ;;
    desktop) echo 2026-11-13T07:07:50Z ;;
    queue) echo 2026-11-12T15:21:00Z ;;
    ios-ui) echo pending ;;
    drops) echo 2026-11-12T15:21:00Z ;;
    *) die "unknown target: $1" ;;
  esac
}

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

# A queue commit's first green merge-gate (its merge-gate lines, `<state>
# <epoch> <description>` oldest first, on stdin): `<seconds from start>
# <epoch>`, or nothing.
queue_seconds() { # start
  awk -v s="$1" '$1 == "success" { print $2 - s, $2; exit }'
}

# A commit's first failing merge-gate status (the same lines on stdin):
# `<epoch> <description>`, or nothing: the queue dropped it.
gate_failure() {
  awk '$1 == "failure" { $1 = ""; sub(/^ /, ""); print; exit }'
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
# infrastructure failure (ADR 0100): a job with no runner, one cancelled
# after it started (not by its own timeout, which merge-gate words as a
# plain failure), or one that never ran.
infra_where() { # where
  [[ $1 =~ ^no\ [A-Za-z]+\ runner || $1 =~ ^cancelled\ on\  || $1 =~ ^did\ not\ run ]]
}

# The test a PC failure status names, after whether it names more: "Failed
# in just test: ( 714/1634) mdoc-markdown from_html::tests::x and 2 more"
# gives `1 mdoc-markdown from_html::tests::x`, one test alone `0 <test>`;
# nothing when it names none.
pc_test() { # description
  local d=$1 more=0
  case "$d" in "Failed in "*": "*) ;; *) return 0 ;; esac
  d=${d#*: }
  [[ ! $d =~ \ and\ [0-9]+\ more$ ]] || more=1
  echo "$more $(sed -E 's/ and [0-9]+ more$//; s/^\( *[0-9]+\/[0-9]+\) //' <<<"$d")"
}

# The slower UI shard's `<seconds> <end epoch>` from a Queue run's jobs.
# Require the complete green pair, or the old single job. A missing, skipped,
# failed or unfinished shard gives no sample; use the latest completion so
# historical windows never see a pair that was only partly finished then.
ios_ui_seconds() {
  awk '{ name = $0; sub(/^[^ ]+ [^ ]+ [^ ]+ /, "", name) }
    name ~ /(^|\/ )ci\/ios \(ui( [12])?\)$/ {
      if ($1 != "success" || $2 <= 0 || $3 <= 0) bad = 1
      if (name ~ /\(ui 1\)$/) one++
      else if (name ~ /\(ui 2\)$/) two++
      else old++
      if ($3 - $2 > slow) slow = $3 - $2
      if ($3 > end) end = $3
    }
    END {
      if (!bad && ((one == 1 && two == 1 && !old) || (old == 1 && !one && !two)))
        print slow + 0, end
    }'
}

# Each started job relevant to the class: `<start> ios|desktop`. A reusable
# workflow prefixes its check name (`ios / ci/ios (ui 1)`, for example).
# Classification uses the job, not whether a gate merely expected it to run.
class_jobs() {
  awk '{ name = $0; sub(/^[^ ]+ [^ ]+ [^ ]+ /, "", name) }
    $1 != "skipped" && $2 > 0 {
      if (name ~ /(^|\/ )ci\/ios( \(|$)/) print $2, "ios"
      if (name ~ /(^|\/ )ci\/(macos|detection)( \(|$)/) print $2, "desktop"
    }'
}

# `<sha> <class>` for every commit in a window, including non-green ones.
# Every commit has a `<created> <sha> <created> other` line, so one that
# started no such job is still counted; a job started after the window's end
# does not change the commit's class in that window.
commit_classes() { # file from to
  awk -v f="$2" -v t="$3" '
    $1 >= f && $1 <= t && $3 <= t {
      seen[$2] = 1
      if ($4 == "ios") ios[$2] = 1
      if ($4 == "desktop") desktop[$2] = 1
    }
    END {
      for (s in seen) {
        c = ios[s] ? (desktop[s] ? "core" : "iOS-only") : (desktop[s] ? "desktop-only" : "other")
        print s, c
      }
    }' "$1" | sort
}

# Whether a test is on the quarantine list: an app UI line (android:, ios:)
# by its name; a Rust line when its nextest filter contains the test's path
# (the last word of `<binary> <path>`) as a whole name, `::` included, and
# names the test's package (its binary's name before any `::`) if it names
# a package.
quarantined() { # test
  [ -f "$quarantine" ] || return 1
  awk -v t="$1" '
    BEGIN {
      app = (t ~ /^(android|ios):/)
      p = t; sub(/.* /, "", p)
      pkg = t; sub(/ .*/, "", pkg); sub(/::.*/, "", pkg)
    }
    /^[ \t]*(#|$)/ { next }
    {
      f = $0; sub(/  +#.*/, "", f); sub(/[ \t]+$/, "", f)
      if (app) { if (f == t) found = 1; next }
      if (f ~ /^(android|ios):/) next
      if (f ~ /package\(/ && index(f, "package(" pkg ")") == 0) next
      s = f
      while ((i = index(s, p)) > 0) {
        c = substr(s, i + length(p), 1)
        b = (i > 1) ? substr(s, i - 1, 1) : ""
        if (c !~ /[A-Za-z0-9_:]/ && b !~ /[A-Za-z0-9_:]/) { found = 1; break }
        s = substr(s, i + 1)
      }
    }
    END { exit !found }' "$quarantine"
}

# Why a drop was not caused by its PR, or `related`: every job its failing
# merge-gate description names is infrastructure, or a PC job naming one
# failing test that is quarantined or failed on another PR's queue commit
# within 14 days. Only PC failures posted by the cutoff count (tests file
# lines: `<epoch> <sha> <pr> <job> <more> <test>`). The first job's rule
# names the drop.
drop_reason() { # sha pr created description tests-file cutoff
  local sha=$1 pr=$2 at=$3 desc=$4 tests=$5 cut=$6 job where line more test why='' rule n=0
  while read -r job where; do
    [ -n "$job" ] || continue
    n=$((n + 1))
    rule=''
    if infra_where "$where"; then
      rule="infrastructure ($job $where)"
    elif [ "$where" = PC ] && { [ "$job" = windows ] || [ "$job" = android ]; }; then
      line=$(awk -v s="$sha" -v j="$job" -v c="$cut" '$2 == s && $4 == j && $1 <= c { $1 = $2 = $3 = $4 = ""; sub(/^ +/, ""); print; exit }' "$tests")
      more=${line%% *} test=${line#* }
      if [ -z "$line" ] || [ "$more" != 0 ]; then
        :
      elif quarantined "$test"; then
        rule="quarantined ($test)"
      elif awk -v p="$pr" -v a="$at" -v r="$repeat_s" -v j="$job" -v t="$test" -v c="$cut" '
        { x = $0; sub(/^[^ ]+ [^ ]+ [^ ]+ [^ ]+ [^ ]+ /, "", x) }
        $3 != p && $4 == j && x == t && $1 <= c && ($1 - a <= r) && (a - $1 <= r) { found = 1; exit }
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
# files in $3, one line each, `<created> <value> <when>` first (when it was
# known): queue (`... <sha>`, seconds to green), ios-ui (seconds), classes
# (`<created> <sha> <job start> ios|desktop|other`, one line per class job
# and one `other` line per commit), drops (`<created> <sha> <pr> <when>
# <description>`) and tests (`<when> <sha> <pr> <job> <more> <test>`, every
# PC failure naming one).
gather() { # from to dir
  local dir=$3 runs id sha at branch pr st gate desc line ctx state t rest jobs
  : >"$dir/queue" && : >"$dir/classes" && : >"$dir/ios-ui" && : >"$dir/drops" && : >"$dir/tests"
  runs=$(runs_between queue.yml "$1" "$2" merge_group) || die "could not list the Queue runs"
  # Each commit once, with its first run.
  while read -r id sha at branch; do
    [ -n "$sha" ] || continue
    pr=$(branch_pr "$branch")
    st=$(fetch_statuses "$sha") || die "could not read the statuses of $sha"
    st=$(sort -s -k3,3n <<<"$st")
    gate=$(awk '$1 == "merge-gate" { $1 = ""; sub(/^ /, ""); print }' <<<"$st")
    line=$(queue_seconds "$at" <<<"$gate")
    [ -z "$line" ] || echo "$at $line $sha" >>"$dir/queue"
    desc=$(gate_failure <<<"$gate")
    [ -z "$desc" ] || echo "$at $sha $pr $desc" >>"$dir/drops"
    while read -r ctx state t rest; do
      case "$ctx $state" in
        "ci/windows failure" | "ci/android failure")
          line=$(pc_test "$rest")
          [ -z "$line" ] || echo "$t $sha $pr ${ctx#ci/} $line" >>"$dir/tests"
          ;;
      esac
    done <<<"$st"
    jobs=$(fetch_jobs "$id") || die "could not read run $id's jobs"
    {
      echo "$at other"
      class_jobs <<<"$jobs"
    } | sed "s/^/$at $sha /" >>"$dir/classes"
    line=$(ios_ui_seconds <<<"$jobs")
    [ -z "$line" ] || echo "$at $line" >>"$dir/ios-ui"
  done < <(sort -k3,3n <<<"$runs" | awk '!seen[$2]++')
}

# --- the report --------------------------------------------------------------

# A sample file's values from commits started from $2 to $3 and known by $3.
samples() { # file from to
  awk -v f="$2" -v t="$3" '$1 >= f && $1 <= t && $3 <= t { print $2 }' "$1"
}

# The epoch from which target $1 reads samples in a window starting at $2,
# then its note for the report line: `<epoch> <note>`.
target_from() { # target window-start
  local date start
  date=$(target_start "$1")
  if [ "$date" = pending ]; then
    echo "$2 not judged: start date pending"
    return
  fi
  start=$(jq -rn --arg s "$date" '$s | fromdateiso8601') || die "invalid start date for $1: $date"
  [ "$start" -ge "$2" ] || start=$2
  echo "$start since $date"
}

# One line per target over the samples from $1 to $2 (the files gather
# wrote in $3), after a line counting the window's commits by class: 0 when
# every judged target holds, 1 when one fails.
report() { # from to dir
  local from=$1 to=$2 dir=$3 rc=0 n m p verdict days per_week target limit what start note
  local at sha pr desc why unrelated=0 drops=0 infra=0 quar=0 rep=0 list=''
  commit_classes "$dir/classes" "$from" "$to" >"$dir/classes.now"
  awk '{ n[$2]++ }
    END { printf "classes  queue commits: %d core, %d desktop-only, %d iOS-only, %d other\n",
      n["core"], n["desktop-only"], n["iOS-only"], n["other"] }' "$dir/classes.now"
  for target in core desktop queue ios-ui; do
    read -r start note <<<"$(target_from "$target" "$from")"
    case "$target" in
      core) limit=$core_min what='core queue commits to green gate' ;;
      desktop) limit=$desktop_min what='desktop-only queue commits to green gate' ;;
      queue) limit=$queue_min what='every queue commit to green gate' ;;
      ios-ui) limit=$ios_ui_min what='green iOS UI runs, the slower shard' ;;
    esac
    if [ "$target" = ios-ui ]; then
      read -r n m p <<<"$(samples "$dir/ios-ui" "$start" "$to" | summary)"
    else
      # queue lines: `<created> <seconds> <when> <sha>`; classes.now: `<sha> <class>`.
      read -r n m p <<<"$(awk -v want="$target" -v f="$start" -v t="$to" '
        FILENAME == ARGV[1] { cls[$1] = $2; next }
        $1 >= f && $1 <= t && $3 <= t && (want == "queue" ||
          (want == "core" && cls[$4] == "core") || (want == "desktop" && cls[$4] == "desktop-only")) { print $2 }
      ' "$dir/classes.now" "$dir/queue" | summary)"
    fi
    verdict=pass
    if ! within "$m" "$limit"; then
      verdict=FAIL
      [ "${note%%:*}" = "not judged" ] || rc=1
    fi
    printf '%-8s %s  %s: %s, median %s min, p90 %s min (target: median %s or less; %s)\n' \
      "$target" "$verdict" "$what" "$n" "$(minutes "$m")" "$(minutes "$p")" "$limit" "$note"
  done
  read -r start note <<<"$(target_from drops "$from")"
  [ "$start" -le "$to" ] || start=$to
  while read -r at sha pr _ desc; do
    [ -n "$sha" ] || continue
    drops=$((drops + 1))
    why=$(drop_reason "$sha" "$pr" "$at" "$desc" "$dir/tests" "$to")
    case "$why" in
      related) continue ;;
      infrastructure*) infra=$((infra + 1)) ;;
      quarantined*) quar=$((quar + 1)) ;;
      *) rep=$((rep + 1)) ;;
    esac
    unrelated=$((unrelated + 1))
    list+="         $(iso "$at") ${sha:0:7} PR #$pr: $why"$'\n'
  done < <(awk -v f="$start" -v t="$to" '$1 >= f && $1 <= t && $4 <= t' "$dir/drops")
  # The rate over the part of the window from the target's start, compared
  # unrounded: a window ending before the start has none and holds.
  days=$(awk -v w=$((to - start)) 'BEGIN { printf "%.1f", w / 86400 }')
  per_week=$(awk -v i="$unrelated" -v w=$((to - start)) 'BEGIN { printf "%.2f", (w > 0 ? i * 7 * 86400 / w : 0) }')
  verdict=pass
  if ! awk -v i="$unrelated" -v w=$((to - start)) -v l="$drops_week" 'BEGIN { exit !(i * 7 * 86400 <= l * w) }'; then
    verdict=FAIL
    [ "${note%%:*}" = "not judged" ] || rc=1
  fi
  echo "drops    $verdict  queue drops not caused by the PR: $unrelated of $drops drops in $days days ($infra infrastructure, $quar quarantined test, $rep repeated test), $per_week a week (target: $drops_week or fewer; $note)"
  printf '%s' "$list"
  if [ -n "$drill" ]; then
    echo "drill    FAIL  forced red by CI_STATS_DRILL, a drill of the nightly's issue"
    rc=1
  fi
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
