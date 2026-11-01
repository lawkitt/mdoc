#!/usr/bin/env bash
# Usage: scripts/local-ci.sh [run|once|install|start|stop|status|uninstall]
#
# Local CI (ADR 0082, ADR 0091). Tests each open non-draft PR from a branch
# in the repository (never forks or bots) at refs/pull/N/merge, in a CI
# checkout that keeps its target/, and posts two commit statuses on the PR
# head: the desktop one (local/macos or local/windows: lint, test, and on
# the Mac the mobile cargo check) and the mobile one (local/ios on the Mac,
# local/android on the PC), each from its own job. A status whose area the
# PR does not touch (scripts/ci-areas.txt) gets success "Not affected" as
# soon as the merge ref is there. Polls every 60 s with this machine's gh
# login; one job at a time; a newer head replaces its PR's queued or running
# jobs. With no PR queued, it tests each new main commit in the areas that
# commit touched and posts the statuses there. Logs, summaries and renders
# stay in $LOCAL_CI_HOME/out for 7 days, served to the tailnet.
#
#   run        the runner loop (what the launchd agent or scheduled task runs)
#   once       the loop until nothing is queued or running, then exit
#   install    macOS: CI checkout, runner copy, launchd agents, tailscale serve
#   start      macOS: load the launchd agents
#   stop       macOS: unload them (a running job stops; it re-runs at start)
#   status     the runner's status file, its agents and recent log
#   uninstall  macOS: stop, remove the agents and the tailnet path
#
# Windows installs with scripts/local-ci-setup.ps1. The installed runner is a
# copy in $LOCAL_CI_HOME/runner, never the CI checkout it switches between
# merge refs; re-run install (or the setup script) to update it. Settings are
# the LOCAL_CI_* variables in config(), also read from $LOCAL_CI_HOME/runner.env.
# The folder this copy runs from, before lib.sh moves to the repository root.
runner_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

agent=com.lawkitt.mdoc.local-ci
web_agent=com.lawkitt.mdoc.local-ci-web
every_area='core desktop recognition ios android'

config() {
  local default_os=
  case "$(platform)" in
    macos-arm64) default_os=macos ;;
    windows-x64) default_os=windows ;;
  esac
  if [ -z "${LOCAL_CI_HOME:-}" ]; then
    case "$default_os" in
      windows) LOCAL_CI_HOME=/e/ci ;;
      *) LOCAL_CI_HOME=$HOME/ci ;;
    esac
  fi
  if [ -f "$LOCAL_CI_HOME/runner.env" ]; then
    set -a
    # shellcheck source=/dev/null
    source "$LOCAL_CI_HOME/runner.env"
    set +a
  fi
  os=${LOCAL_CI_OS:-$default_os}
  [ -n "$os" ] || die "local CI runs on macOS or Windows (this is $(platform)); set LOCAL_CI_OS to test elsewhere"
  # The desktop status and the mobile one (ADR 0091): the Mac runs the iOS
  # job, the PC the Android job.
  context=local/$os
  case "$os" in
    macos) mobile_area=ios ;;
    windows) mobile_area=android ;;
    *) mobile_area=${LOCAL_CI_MOBILE_AREA:-ios} ;;
  esac
  mobile_context=local/$mobile_area
  repo=${LOCAL_CI_REPO:-lawkitt/mdoc}
  checkout=$LOCAL_CI_HOME/mdoc
  out=$LOCAL_CI_HOME/out
  state=$LOCAL_CI_HOME/state
  url=${LOCAL_CI_URL:-}
  port=${LOCAL_CI_PORT:-8787}
  poll_s=${LOCAL_CI_POLL:-60}
  tick_s=${LOCAL_CI_TICK:-5}
  # setup installs the merge ref's pinned tools, as hosted CI's setup step
  # does. The Mac's desktop job also checks the mobile core (`ffi`, ADR
  # 0085) and runs the detection gate (`detect`, ADR 0093); the mobile job
  # is the iOS job (`ios`) or the Android job (`android`, ADR 0089).
  case "$os" in
    macos) steps=${LOCAL_CI_STEPS:-setup lint test ffi detect} ;;
    *) steps=${LOCAL_CI_STEPS:-setup lint test} ;;
  esac
  # A change only to the recognition fixtures (area recognition): the Mac
  # runs mdoc-detect's tests and the detection gate, not the whole suite;
  # other runners leave it to the Mac.
  case "$os" in
    macos) recognition_steps=${LOCAL_CI_RECOGNITION_STEPS:-setup recognition detect} ;;
    *) recognition_steps='' ;;
  esac
  # A head the hosted fallback passed: hosted CI ran clippy and test, so the
  # Mac runs only what hosted CI lacks, the detection gate (it skips itself
  # when recognition did not change); other runners do not test it again.
  case "$os" in
    macos) leftover_steps=${LOCAL_CI_LEFTOVER_STEPS:-detect} ;;
    *) leftover_steps='' ;;
  esac
  mobile_steps=${LOCAL_CI_MOBILE_STEPS:-setup $mobile_area}
  # The hosted job limits (ADR 0082): clippy 20 min, test 30 min; the mobile
  # cargo check shares lint's. The iOS job builds the core and app and runs
  # the Simulator tests (ADR 0085): 40 min.
  lint_s=${LOCAL_CI_LINT_TIMEOUT:-1200}
  test_s=${LOCAL_CI_TEST_TIMEOUT:-1800}
  mobile_s=${LOCAL_CI_MOBILE_TIMEOUT:-2400}
  # The Android job builds the core for two ABIs and runs the emulator tests
  # (ADR 0089): 40 min.
  android_s=${LOCAL_CI_ANDROID_TIMEOUT:-2400}
  keep_days=${LOCAL_CI_KEEP_DAYS:-7}
  # After a long time off, only the newest main commits are tested.
  main_max=${LOCAL_CI_MAIN_MAX:-10}
  # The Mac is a build slot beside two agent builders (ADR 0082): a job
  # starts at or under this kern.memorystatus_vm_pressure_level (1 normal,
  # 2 warning, 4 critical) and with this much disk free. Not swap in use:
  # macOS keeps swap after the memory is free, which held jobs for hours.
  min_free_gb=${LOCAL_CI_MIN_FREE_GB:-40}
  max_pressure=${LOCAL_CI_MAX_PRESSURE:-1}
}

log() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }

short() { printf '%s' "${1:0:7}"; }

duration() { printf '%dm%02ds' $(($1 / 60)) $(($1 % 60)); }

# --- areas (scripts/ci-areas.txt, ADR 0091) ----------------------------------

# The area a status stands for.
area_of() {
  if [ "$1" = "$context" ]; then echo desktop; else echo "$mobile_area"; fi
}

# A job's steps: the context's, fewer for a recognition-only change on the
# desktop status, and only the leftover ones on a head the fallback passed.
steps_of() { # context [areas [state description]]
  if [ "$1" != "$context" ]; then
    echo "$mobile_steps"
  elif [ "${3:-}" = SUCCESS ] && [[ ${4:-} == "Hosted fallback"* ]]; then
    echo "$leftover_steps"
  elif [ $# -ge 2 ] && ! affects desktop "$2" && affects recognition "$2"; then
    echo "$recognition_steps"
  else
    echo "$steps"
  fi
}

# The areas a commit range touches, by the area table in commit $1 (every
# area when it has none): a PR's changes since it left main, or a main
# commit's since the one before.
commit_areas() { # commit range
  local table
  if ! table=$(git -C "$checkout" show "$1:scripts/ci-areas.txt" 2>/dev/null); then
    echo "$every_area"
    return 0
  fi
  git -C "$checkout" diff --no-renames --name-only -z "$2" | changed_areas "$table"
}

# What a status says when its job need not run: docs only, or no change in
# its area. False when the job runs.
settled_text() { # context areas on
  local area
  area=$(area_of "$1")
  if [ -z "$2" ]; then
    echo "Docs only, not tested ($3)"
  elif [ "$area" = desktop ] && ! affects desktop "$2" && affects recognition "$2"; then
    [ -z "$recognition_steps" ] || return 1
    echo "Not affected: recognition fixtures only, the Mac tests them ($3)"
  elif ! affects "$area" "$2"; then
    echo "Not affected: no core or $area change ($3)"
  else
    return 1
  fi
}

# --- GitHub -----------------------------------------------------------------

# One line per open PR: number, draft, cross-repository, author type,
# mergeable, base, head, then the desktop and the mobile status on the head:
# state (or NONE), when it was posted and its description.
# shellcheck disable=SC2016 # GraphQL variables, not shell ones
poll_query='query($owner: String!, $name: String!, $desktop: String!, $mobile: String!) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 50) {
      nodes {
        number isDraft isCrossRepository mergeable baseRefName headRefOid
        author { __typename }
        commits(last: 1) { nodes { commit { status {
          desktop: context(name: $desktop) { state createdAt description }
          mobile: context(name: $mobile) { state createdAt description }
        } } } }
      }
    }
  }
}'
# shellcheck disable=SC2016 # jq variables, not shell ones
poll_jq='.data.repository.pullRequests.nodes[] | (.commits.nodes[0].commit.status // {}) as $s
  | [.number, .isDraft, .isCrossRepository, (.author.__typename // "None"), .mergeable,
  .baseRefName, .headRefOid,
  ($s.desktop.state // "NONE"), ($s.desktop.createdAt // ""), ($s.desktop.description // ""),
  ($s.mobile.state // "NONE"), ($s.mobile.createdAt // ""), ($s.mobile.description // "")] | @tsv'

# Sets prs to the rows this runner tests, one per PR and status, from PRs
# that are not drafts, from branches in the repository, opened by a user
# (Dependabot is a bot). Row fields: number, draft, cross-repository, author
# type, mergeable, base, head, context, state, time, description; the
# context comes before the fields that can be empty (read merges empty
# tab-separated fields).
poll() {
  local lines
  if ! lines=$(gh api graphql -f owner="${repo%/*}" -f name="${repo#*/}" \
    -f desktop="$context" -f mobile="$mobile_context" -f query="$poll_query" --jq "$poll_jq"); then
    log "warning: could not list PRs; retrying at the next poll"
    return 1
  fi
  prs=$(awk -F'\t' -v OFS='\t' -v d="$context" -v m="$mobile_context" '
    $2 == "false" && $3 == "false" && $4 == "User" {
      print $1, $2, $3, $4, $5, $6, $7, d, $8, $9, $10
      print $1, $2, $3, $4, $5, $6, $7, m, $11, $12, $13
    }' <<<"$lines")
  polled=true
}

# Sets post_ok to whether GitHub took it.
post() { # context sha state description [target url]
  local args=(api -X POST "repos/$repo/statuses/$2" -f "state=$3" -f "context=$1"
    -f "description=${4:0:140}")
  [ -z "${5:-}" ] || args+=(-f "target_url=$5")
  # A lost final status would leave the head pending and run it again.
  local try
  post_ok=true
  for try in 1 2 3; do
    if gh "${args[@]}" >/dev/null; then
      log "$3 $1 on $(short "$2"): $4"
      return 0
    fi
    [ "$try" -eq 3 ] || sleep "$tick_s"
  done
  post_ok=false
  log "warning: could not post $3 $1 on $(short "$2"): $4"
}

pr_line() { awk -F'\t' -v n="$1" '$1 == n { print; exit }' <<<"$prs"; }

drop_row() { prs=$(awk -F'\t' -v n="$1" -v c="$2" '!($1 == n && $8 == c)' <<<"$prs"); }

# Rows waiting for this runner: no status yet (posted "Queued" by
# queue_new), pending (queued, or left by a stopped runner), or a status
# from the hosted fallback (scripts/local-ci-fallback.sh) that a local job
# should replace: an error (hosted CI gave up), a mobile row's "NOT tested",
# and on the Mac a desktop success, for the detection gate hosted CI lacks
# (leftover_steps). Not one the fallback is running hosted CI for, nor its
# failures, nor (off the Mac) its desktop successes: hosted CI took those
# heads so this queue would not. PRs by their oldest waiting status, a PR's desktop row
# before its mobile one even when the desktop status is newer (a step it
# posted before a restart). Each line gains two fields: 0 (desktop) or 1,
# and the PR's oldest status time.
queue() {
  awk -F'\t' -v OFS='\t' -v rn="${run_pr:-}" -v rc="${run_ctx:-}" -v d="$context" -v left="$leftover_steps" '
    function wanted(fb) {
      fb = ($11 ~ /^Hosted fallback/)
      if ($9 == "NONE") return 1
      if (!fb) return $9 == "PENDING"
      if ($9 == "ERROR") return 1
      if ($9 != "SUCCESS") return 0
      return $8 != d || left != ""
    }
    !($1 == rn && $8 == rc) && wanted() {
      line[++rows] = $0; pr[rows] = $1; key[rows] = ($8 == d ? 0 : 1)
      if (!($1 in first) || $10 < first[$1]) first[$1] = $10
    }
    END { for (i = 1; i <= rows; i++) print line[i], key[i], first[pr[i]] }' <<<"$prs" |
    sort -t$'\t' -k13,13 -k1,1n -k12,12n
}

# The contexts of PR $1's queued rows.
queued_contexts() { queue | awk -F'\t' -v n="$1" '$1 == n { print $8 }'; }

# What a queued head's status says: "Queued", or why nothing starts, so a
# stalled runner shows on GitHub, and the hosted fallback acts sooner
# (scripts/local-ci-fallback.sh). Stable while the reason lasts, so it is
# posted once, not at every poll.
queued_text() {
  case "${waiting_kind:-}" in
    memory) echo "Queued; waiting for memory pressure to drop (now $(pressure_name "$pressure"))" ;;
    disk) echo "Queued; waiting for disk space (starts at $min_free_gb GB free)" ;;
    *) echo "Queued" ;;
  esac
}

# Posts a pending status on each new row, and the current queued text on
# queued rows whose status says otherwise. Rows the hosted fallback has
# taken ("Hosted fallback: ...") keep their status.
queue_new() {
  local now n head ctx text
  now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  text=$(queued_text)
  while IFS=$'\t' read -r n _ _ _ _ _ head ctx _; do
    post "$ctx" "$head" pending "$text" "${url:+$url/status.txt}"
    prs=$(awk -F'\t' -v OFS='\t' -v n="$n" -v c="$ctx" -v t="$now" -v d="$text" \
      '$1 == n && $8 == c { $9 = "PENDING"; $10 = t; $11 = d } { print }' <<<"$prs")
  done < <(awk -F'\t' -v rn="${run_pr:-}" -v rc="${run_ctx:-}" -v text="$text" '!($1 == rn && $8 == rc) &&
    ($9 == "NONE" || ($9 == "PENDING" && $11 ~ /^Queued/ && $11 != text))' <<<"$prs")
}

# Settles PR $1's queued rows that need no build: a conflict fails them, a
# docs-only PR or one outside a row's area passes it. Sets settle_merge and
# settle_base_sha to the merge ref it fetched. False while GitHub has no
# merge ref at the head yet.
settle() { # pr head mergeable base
  local n=$1 head=$2 ctx text areas
  case "$3" in
    CONFLICTING)
      for ctx in $(queued_contexts "$n"); do
        post "$ctx" "$head" failure "Conflicts with $4"
        drop_row "$n" "$ctx"
      done
      return 0
      ;;
    MERGEABLE) ;;
    *) return 1 ;; # GitHub is still computing the merge
  esac
  if ! git -C "$checkout" fetch --quiet --no-tags --force origin \
    "+refs/pull/$n/merge:refs/local-ci/merge"; then
    log "warning: could not fetch refs/pull/$n/merge"
    return 1
  fi
  settle_merge=$(git -C "$checkout" rev-parse refs/local-ci/merge)
  if [ "$(git -C "$checkout" rev-parse --verify --quiet "$settle_merge^2")" != "$head" ]; then
    log "#$n: merge ref not yet at $(short "$head")"
    return 1
  fi
  settle_base_sha=$(git -C "$checkout" rev-parse "$settle_merge^1")
  if ! areas=$(commit_areas "$settle_merge" "$settle_merge^1...$settle_merge^2"); then
    log "warning: #$n: could not read its areas; running every job"
    areas=$every_area
  fi
  settle_areas=$areas
  for ctx in $(queued_contexts "$n"); do
    text=$(settled_text "$ctx" "$areas" "$4@$(short "$settle_base_sha")") || continue
    post "$ctx" "$head" success "$text"
    drop_row "$n" "$ctx"
  done
}

# Settles new heads at once, while a job runs or the Mac waits for
# capacity, so a status outside the PR's areas does not wait in the queue.
# Each head is settled once; start_next settles again before a job starts.
settle_new() {
  local n mergeable base head
  while IFS=$'\t' read -r n _ _ _ mergeable base head _; do
    ! grep -qxF "$head" <<<"$settled" || continue
    settle "$n" "$head" "$mergeable" "$base" || continue
    settled=$(printf '%s\n' "$head" "$settled" | head -n 200)
  done < <(queue | awk -F'\t' '!seen[$1]++')
}

# --- the job ----------------------------------------------------------------

memory_pressure() { sysctl -n kern.memorystatus_vm_pressure_level 2>/dev/null || echo 1; }

disk_free_gb() { df -Pk "$LOCAL_CI_HOME" | awk 'NR == 2 { print int($4 / 1048576) }'; }

pressure_name() {
  case "$1" in
    1) echo normal ;;
    2) echo warning ;;
    4) echo critical ;;
    *) echo "level $1" ;;
  esac
}

# Sets waiting (for status.txt) and waiting_kind (memory, disk or empty);
# logs when the reason changes. The next poll posts it on queued heads.
capacity_ok() {
  local was=${waiting_kind:-} free_gb
  waiting='' waiting_kind=''
  if [ "$os" = macos ]; then
    pressure=$(memory_pressure)
    free_gb=$(disk_free_gb)
    if [ "${pressure:-1}" -gt "$max_pressure" ]; then
      waiting_kind=memory
      waiting="memory pressure $(pressure_name "$pressure") (starts at $(pressure_name "$max_pressure") or lower)"
    elif [ "${free_gb:-0}" -lt "$min_free_gb" ]; then
      waiting_kind=disk
      waiting="${free_gb} GB free (starts at $min_free_gb GB)"
    fi
  fi
  if [ "$waiting_kind" != "$was" ]; then
    if [ -n "$waiting_kind" ]; then
      log "waiting: $waiting"
    else
      log "capacity back: $was no longer holds jobs"
    fi
  fi
  [ -z "$waiting_kind" ]
}

timeout_for() {
  case "$1" in
    setup | lint | ffi) echo "$lint_s" ;;
    ios) echo "$mobile_s" ;;
    android) echo "$android_s" ;;
    *) echo "$test_s" ;;
  esac
}

# How a step reads in logs and statuses.
label() {
  case "$1" in
    setup) echo scripts/setup.sh ;;
    ffi) echo "just mobile --ffi" ;;
    ios) echo "just mobile --ios" ;;
    detect) echo "just detection-gate" ;;
    recognition) echo "cargo nextest run -p mdoc-detect" ;;
    *) echo "just $1" ;;
  esac
}

# GitHub can build a PR's merge ref on a main from before the mobile check
# (ADR 0085). The merge's first parent is that main: when it had the check,
# the step runs, so a PR that removes the script fails rather than skips.
mobile_check_expected() {
  git cat-file -e HEAD^1:scripts/mobile-check.sh 2>/dev/null
}

# The same for the Android job (ADR 0089).
android_check_expected() {
  git cat-file -e HEAD^1:scripts/android-check.sh 2>/dev/null
}

# The same for the detection gate (ADR 0093).
detection_gate_expected() {
  git cat-file -e HEAD^1:scripts/detection-gate.sh 2>/dev/null
}

# Runs in the background in its own process group; output goes to log.txt.
# Exit 0 passed, 1 a step failed (named in .step), 3 the checkout failed.
# The runner has settled the areas, so the mobile jobs always run.
job() { # commit dir step...
  set +m
  local commit=$1 dir=$2 step start rc
  shift 2
  cd "$checkout" || exit 3
  unset CARGO_TARGET_DIR RUSTFLAGS MDOC_HEADLESS_ARTIFACTS
  echo "local-ci: checkout $commit"
  git checkout --quiet --force --detach "$commit" || exit 3
  # Keep the build cache. Git Bash rewrites a native program's argument that
  # starts with / as a Windows path (/target/ -> C:/Program Files/Git/target/),
  # which matched nothing and made every Windows job a cold build.
  MSYS_NO_PATHCONV=1 git clean -ffdxq -e /target/ || exit 3
  for step in "$@"; do
    start=$(date +%s)
    printf '%s %s\n' "$step" "$start" >"$dir/.step"
    echo "local-ci: $(label "$step")"
    rc=0
    case "$step" in
      # Not through just: setup may replace the just binary.
      setup) bash scripts/setup.sh </dev/null || rc=$? ;;
      test) MDOC_HEADLESS_ARTIFACTS=$dir/renders just "$step" || rc=$? ;;
      # The recognition fixtures' tests, without the rest of the suite.
      recognition) cargo nextest run --locked -p mdoc-detect || rc=$? ;;
      ffi | ios)
        if mobile_check_expected; then
          MDOC_IOS_JOB=always just mobile "--$step" || rc=$?
        else
          echo "local-ci: this commit's main predates the mobile check; skipped"
        fi
        ;;
      detect)
        if detection_gate_expected; then
          just detection-gate || rc=$?
        else
          echo "local-ci: this commit's main predates the detection gate; skipped"
        fi
        ;;
      android)
        if android_check_expected; then
          MDOC_ANDROID_JOB=always just "$step" || rc=$?
        else
          echo "local-ci: this commit's main predates the Android job; skipped"
        fi
        ;;
      *) just "$step" || rc=$? ;;
    esac
    if [ "$rc" -eq 0 ]; then
      echo "$(label "$step"): passed in $(duration $(($(date +%s) - start)))" >>"$dir/summary.txt"
    else
      echo "$(label "$step"): failed (exit $rc) after $(duration $(($(date +%s) - start)))" >>"$dir/summary.txt"
      exit 1
    fi
  done
  exit 0
}

# A PR's job (kind: its number) or a main commit's (kind: main).
start_job() { # kind head base base-sha context commit steps
  local kind=$1 head=$2 ctx=$5 name dir area link
  area=$(area_of "$ctx")
  if [ "$kind" = main ]; then
    name=main
    link=https://github.com/$repo/commit/$head
  else
    name=pr-$kind
    link=https://github.com/$repo/pull/$kind
  fi
  dir=$name/$(short "$head")
  [ "$area" = desktop ] || dir+=-$area
  rm -rf "${out:?}/$dir"
  # A macOS render set is about 0.5 GB: keep it for a PR's (or main's)
  # newest head only.
  [ "$area" != desktop ] || rm -rf "$out/$name"/*/renders
  mkdir -p "$out/$dir"
  run_kind=$kind run_pr='' run_head=$head run_ctx=$ctx run_base=$3 run_base_sha=$4
  run_dir=$out/$dir run_start=$(date +%s)
  [ "$kind" = main ] || run_pr=$kind
  run_url=
  [ -z "$url" ] || run_url=$url/$dir/
  cat >"$run_dir/summary.txt" <<EOF
$ctx: $link
head: $head
base: $3 $4
started: $(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF
  post "$ctx" "$head" pending "Running on $3@$(short "$4")" "$run_url"
  set -m
  echo "steps: $7" >>"$run_dir/summary.txt"
  # shellcheck disable=SC2086 # the steps are words
  job "$6" "$run_dir" $7 </dev/null >"$run_dir/log.txt" 2>&1 &
  run_pid=$!
  set +m
  echo "$run_pid" >"$state/job.pid"
  log "started $(run_name) $ctx on $3@$(short "$4") (pid $run_pid)"
}

run_name() {
  if [ "$run_kind" = main ]; then echo "main $(short "$run_head")"; else echo "#$run_pr $(short "$run_head")"; fi
}

# Stops the job's whole process group (cargo, rustc, test binaries).
kill_job() {
  local pid=$1 w
  if [ "$os" = windows ] && command -v taskkill >/dev/null; then
    # Git Bash: native children (cargo.exe, rustc.exe) are not in the MSYS
    # process group, so end each group member's Windows process tree.
    for w in $(ps | awk -v g="$pid" 'NR > 1 { if ($1 !~ /^[0-9]+$/) $1 = ""; $0 = $0; if ($3 == g) print $4 }'); do
      taskkill //F //T //PID "$w" >/dev/null 2>&1 || true
    done
  fi
  kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    kill -0 -- "-$pid" 2>/dev/null || break
    sleep 1
  done
  kill -KILL -- "-$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

finish() { # state description
  post "$run_ctx" "$run_head" "$1" "$2" "$run_url"
  echo "result: $1, $2" >>"$run_dir/summary.txt"
  rm -f "$state/job.pid" "$run_dir/.step"
  # A main job whose result GitHub did not take stays queued and runs
  # again; a PR's stays pending on GitHub, which queues it again.
  if [ "$run_kind" = main ] && $post_ok && [ -f "$state/main-queue" ]; then
    awk -F'\t' -v s="$run_head" -v c="$run_ctx" '!($1 == s && $2 == c)' "$state/main-queue" >"$state/main-queue.new"
    mv -f "$state/main-queue.new" "$state/main-queue"
  fi
  run_kind='' run_pr='' run_head='' run_ctx='' run_pid='' run_step='' run_note=''
  # Statuses changed: read them again before starting the next job.
  prs=
  next_poll=0
}

check_job() {
  local step since limit rc=0 on
  on="$run_base@$(short "$run_base_sha")"
  if kill -0 "$run_pid" 2>/dev/null; then
    read -r step since 2>/dev/null <"$run_dir/.step" || return 0
    # Each step posts its own pending status: GitHub shows the step, and a
    # status older than the longest step limit means this runner stopped
    # (scripts/local-ci-fallback.sh).
    if [ "$step" != "${run_step:-}" ]; then
      run_step=$step
      post "$run_ctx" "$run_head" pending "Running $(label "$step") on $on" "$run_url"
    fi
    limit=$(timeout_for "$step")
    if [ $(($(date +%s) - since)) -gt "$limit" ]; then
      log "$(run_name) $run_ctx: $(label "$step") passed its $((limit / 60)) min limit"
      echo "local-ci: $(label "$step") timed out after $(duration "$limit")" >>"$run_dir/log.txt"
      kill_job "$run_pid"
      finish failure "Timed out in $(label "$step") after $(duration "$limit") on $on"
    fi
    return 0
  fi
  wait "$run_pid" || rc=$?
  case "$rc" in
    0) finish success "Passed on $on in $(duration $(($(date +%s) - run_start)))${run_note:-}" ;;
    1)
      read -r step _ 2>/dev/null <"$run_dir/.step" || step='?'
      finish failure "Failed in $(label "$step") on $on"
      ;;
    *) finish error "Runner error (exit $rc) on $on; see the log" ;;
  esac
}

# A newer head replaces a running PR job; a closed or drafted PR ends it. A
# main job always runs to the end.
sync_running() {
  [ "${run_kind:-}" != '' ] && [ "$run_kind" != main ] || return 0
  local line head
  line=$(pr_line "$run_pr")
  head=$(cut -f7 <<<"$line")
  if [ -z "$line" ]; then
    kill_job "$run_pid"
    finish error "Stopped: PR closed or marked draft"
  elif [ "$head" != "$run_head" ]; then
    kill_job "$run_pid"
    finish error "Superseded by $(short "$head")"
  fi
}

# Starts the first startable queued row, settling those that need no build;
# with no PR row queued, the next main commit's. Returns 0 if a job started.
start_next() {
  local n mergeable base head ctx row_state row
  capacity_ok || return 1
  while IFS=$'\t' read -r n _ _ _ mergeable base head ctx row_state _; do
    settle "$n" "$head" "$mergeable" "$base" || continue
    # Settled without a build.
    row=$(queue | awk -F'\t' -v n="$n" -v c="$ctx" '$1 == n && $8 == c')
    [ -n "$row" ] || continue
    start_job "$n" "$head" "$base" "$settle_base_sha" "$ctx" "$settle_merge" \
      "$(steps_of "$ctx" "$settle_areas" "$row_state" "$(cut -f11 <<<"$row")")"
    # A head the fallback passed: its result keeps saying what hosted CI ran.
    if [ "$ctx" = "$context" ] && [ "$row_state" = SUCCESS ]; then
      run_note="; clippy and test by hosted CI"
    fi
    return 0
  done < <(queue)
  [ -z "$(queue)" ] || return 1
  start_main
}

# --- main (ADR 0091) ---------------------------------------------------------

# Main commits' jobs wait in $state/main-queue ("commit<TAB>context" lines),
# which outlives a restart, behind every PR row; $state/main-last is the
# newest main commit queued or settled.

# Settles or queues the next main commits, oldest first, until a job is
# queued or main is done. False when nothing is queued.
queue_main() {
  local tip last next ctx text areas commits skipped
  if ! git -C "$checkout" fetch --quiet --no-tags --force origin "+refs/heads/main:refs/local-ci/main"; then
    log "warning: could not fetch main"
    return 1
  fi
  check_copy
  tip=$(git -C "$checkout" rev-parse refs/local-ci/main)
  while [ ! -s "$state/main-queue" ]; do
    last=$(cat "$state/main-last" 2>/dev/null || true)
    [ "$last" != "$tip" ] || return 1
    if [ -n "$last" ] && git -C "$checkout" merge-base --is-ancestor "$last" "$tip" 2>/dev/null; then
      commits=$(git -C "$checkout" rev-list --first-parent --reverse "$last..$tip")
      skipped=$(($(wc -l <<<"$commits") - main_max))
      if [ "$skipped" -gt 0 ]; then
        log "main: $skipped older commits not tested (LOCAL_CI_MAIN_MAX=$main_max)"
        commits=$(tail -n "$main_max" <<<"$commits")
      fi
      next=$(head -n 1 <<<"$commits")
    else
      # The first run, or main was rewritten: its tip only.
      next=$tip
    fi
    areas=$(commit_areas "$next" "$next^1..$next" 2>/dev/null) || areas=$every_area
    # Settled statuses first, then the jobs in one rename, then the cursor:
    # a stop at any point repeats the commit rather than skips it.
    : >"$state/main-queue.new"
    for ctx in "$context" "$mobile_context"; do
      if text=$(settled_text "$ctx" "$areas" "main@$(short "$next")"); then
        post "$ctx" "$next" success "$text"
      else
        printf '%s\t%s\n' "$next" "$ctx" >>"$state/main-queue.new"
      fi
    done
    mv -f "$state/main-queue.new" "$state/main-queue"
    echo "$next" >"$state/main-last.new"
    mv -f "$state/main-last.new" "$state/main-last"
    while IFS=$'\t' read -r _ ctx; do
      post "$ctx" "$next" pending Queued "${url:+$url/status.txt}"
    done <"$state/main-queue"
  done
}

# The installed runner is a copy that only install (or the Windows setup
# script) updates: sets outdated, for status.txt, to the files that differ
# from main's, and logs when that changes. Line endings are ignored (a
# Windows checkout may have CRLF).
check_copy() {
  local f was=${outdated:-}
  outdated=''
  for f in local-ci.sh lib.sh; do
    git -C "$checkout" cat-file -e "refs/local-ci/main:scripts/$f" 2>/dev/null || continue
    cmp -s <(git -C "$checkout" show "refs/local-ci/main:scripts/$f" | tr -d '\r') \
      <(tr -d '\r' <"$runner_dir/$f" 2>/dev/null) || outdated+="${outdated:+ and }$f"
  done
  if [ "$outdated" != "$was" ] && [ -n "$outdated" ]; then
    log "warning: this runner's $outdated differ from main's; re-run install (or local-ci-setup.ps1)"
  fi
}

# Starts the next main job, after a poll found no PR row waiting.
start_main() {
  local commit ctx areas
  if [ ! -s "$state/main-queue" ]; then
    $main_due || return 1
    main_due=false
    queue_main || return 1
  fi
  IFS=$'\t' read -r commit ctx <"$state/main-queue"
  areas=$(commit_areas "$commit" "$commit^1..$commit" 2>/dev/null) || areas=$every_area
  start_job main "$commit" main "$commit" "$ctx" "$commit" "$(steps_of "$ctx" "$areas")"
}

prune() {
  [ -d "$out" ] || return 0
  find "$out" -mindepth 2 -maxdepth 2 -type d -mtime +$((keep_days - 1)) -exec rm -rf {} + 2>/dev/null || true
  find "$out" -mindepth 1 -maxdepth 1 -type d -empty -exec rmdir {} + 2>/dev/null || true
}

write_status() {
  local next=$out/.status.txt q mq
  q=$(queue | awk -F'\t' '{ printf "%s#%s %s %s", (NR > 1 ? ", " : ""), $1, substr($7, 1, 7), $8 }')
  mq=$(awk -F'\t' '{ printf "%s%s %s", (NR > 1 ? ", " : ""), substr($1, 1, 7), $2 }' "$state/main-queue" 2>/dev/null || true)
  {
    echo "$context and $mobile_context runner on $(hostname)"
    echo "updated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    if [ -n "${run_pid:-}" ]; then
      local step='?' since
      read -r step since 2>/dev/null <"$run_dir/.step" || since=$run_start
      echo "running: $(run_name) $run_ctx on $run_base@$(short "$run_base_sha"), $(label "$step") for $(duration $(($(date +%s) - since))) ${run_url:-$run_dir}"
    else
      echo "running: nothing"
    fi
    echo "queue: ${q:-empty}"
    echo "main queue: ${mq:-empty}"
    [ -z "${outdated:-}" ] || echo "outdated: this runner's $outdated differ from main's; re-run install"
    [ -z "${waiting:-}" ] || echo "waiting: $waiting"
  } >"$next" && mv -f "$next" "$out/status.txt"
}

# True when $1 is a live runner or job, not a pid reused after a restart.
is_runner() {
  [ -n "$1" ] && kill -0 "$1" 2>/dev/null || return 1
  if [ "$os" = windows ]; then
    ps -p "$1" 2>/dev/null | grep -q '/bash'
  else
    ps -p "$1" -o args= 2>/dev/null | grep -q 'local-ci\.sh'
  fi
}

lock() {
  local pid
  if ! mkdir "$state/lock" 2>/dev/null; then
    pid=$(cat "$state/lock/pid" 2>/dev/null || true)
    if is_runner "$pid"; then
      die "a runner is already running (pid $pid)"
    fi
    rm -rf "$state/lock"
    mkdir "$state/lock"
  fi
  echo $$ >"$state/lock/pid"
}

# A job left by a runner that died would build beside the new one.
kill_stale_job() {
  local pid
  pid=$(cat "$state/job.pid" 2>/dev/null || true)
  if is_runner "$pid"; then
    log "stopping job $pid left by the previous runner"
    kill_job "$pid"
  fi
  rm -f "$state/job.pid"
}

# shellcheck disable=SC2329 # the EXIT trap
on_exit() {
  if [ -n "${run_pid:-}" ]; then
    log "stopping; $(run_name) $run_ctx re-runs when the runner starts again"
    kill_job "$run_pid"
    rm -f "$state/job.pid"
  fi
  rm -rf "$state/lock"
}

cmd_run() { # [once]
  local once=${1:-}
  [ -d "$checkout/.git" ] || die "no CI checkout at $checkout; run install first"
  mkdir -p "$out" "$state"
  lock
  if [ -z "$once" ] && [ -z "${LOCAL_CI_FOREGROUND:-}" ]; then
    echo "local CI ($context, $mobile_context): logging to $state/runner.log"
    exec >>"$state/runner.log" 2>&1
  fi
  trap on_exit EXIT
  trap 'exit 0' TERM INT HUP
  log "runner started: $context and $mobile_context for $repo, checkout $checkout"
  kill_stale_job
  prs='' polled=false run_kind='' run_pr='' run_head='' run_ctx='' run_pid='' run_step=''
  waiting='' waiting_kind='' next_poll=0 settled='' main_due=false outdated='' post_ok=true
  local now
  while :; do
    [ -z "$run_pid" ] || check_job
    now=$(date +%s)
    if [ "$now" -ge "$next_poll" ]; then
      next_poll=$((now + poll_s))
      if poll; then
        sync_running
        # A replaced job changed statuses: read them again at once.
        [ "$next_poll" != 0 ] || continue
        settle_new
        queue_new
        main_due=true
      fi
      prune
    fi
    [ -n "$run_pid" ] || start_next || true
    write_status
    if [ -n "$once" ] && $polled && [ -z "$run_pid" ] && [ "$next_poll" != 0 ] &&
      [ -z "$(queue)" ] && [ ! -s "$state/main-queue" ] && ! $main_due; then
      log "queue empty"
      return 0
    fi
    sleep "$tick_s"
  done
}

# --- macOS install ------------------------------------------------------------

plist_path() { echo "$HOME/Library/LaunchAgents/$1.plist"; }

write_plists() {
  local bash_path
  bash_path=$(command -v bash)
  cat >"$(plist_path "$agent")" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$agent</string>
  <key>ProgramArguments</key>
  <array>
    <string>$bash_path</string>
    <string>$LOCAL_CI_HOME/runner/scripts/local-ci.sh</string>
    <string>run</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>LOCAL_CI_HOME</key><string>$LOCAL_CI_HOME</string>
    <key>PATH</key><string>$HOME/.cargo/bin:$PATH</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ThrottleInterval</key><integer>30</integer>
  <key>StandardOutPath</key><string>$state/launchd.log</string>
  <key>StandardErrorPath</key><string>$state/launchd.log</string>
</dict>
</plist>
EOF
  cat >"$(plist_path "$web_agent")" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$web_agent</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/bin/python3</string>
    <string>-m</string>
    <string>http.server</string>
    <string>--bind</string>
    <string>127.0.0.1</string>
    <string>$port</string>
    <string>--directory</string>
    <string>$out</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>/dev/null</string>
  <key>StandardErrorPath</key><string>/dev/null</string>
</dict>
</plist>
EOF
}

require_macos() {
  [ "$os" = macos ] || die "$1 is for macOS; on Windows use scripts/local-ci-setup.ps1"
}

loaded() { launchctl print "gui/$(id -u)/$1" >/dev/null 2>&1; }

cmd_start() {
  require_macos start
  local a try
  for a in "$web_agent" "$agent"; do
    [ -f "$(plist_path "$a")" ] || die "$a is not installed; run install"
    loaded "$a" && continue
    for try in 1 2 3 4 5; do
      launchctl bootstrap "gui/$(id -u)" "$(plist_path "$a")" 2>/dev/null && break
      sleep 2
    done
    loaded "$a" || die "could not load $a"
  done
  echo "started $agent and $web_agent"
}

# bootout returns before the agent has exited; wait for it, so a start right
# after does not fail.
cmd_stop() {
  require_macos stop
  local a
  for a in "$agent" "$web_agent"; do
    launchctl bootout "gui/$(id -u)/$a" 2>/dev/null || true
    for _ in $(seq 1 30); do
      loaded "$a" || break
      sleep 1
    done
    ! loaded "$a" || die "$a did not stop"
  done
  echo "stopped $agent and $web_agent"
}

# runner.env: install's two lines, then the machine's own settings (say
# LOCAL_CI_MIN_FREE_GB=30), which a re-install keeps. Never edit the plist:
# install rewrites it.
write_runner_env() { # url
  local file=$LOCAL_CI_HOME/runner.env kept=''
  [ ! -f "$file" ] || kept=$(grep -vE '^(# Written by scripts/local-ci|LOCAL_CI_URL=|LOCAL_CI_PORT=)' "$file" || true)
  {
    echo "# Written by scripts/local-ci.sh install, which keeps the lines after these three."
    printf 'LOCAL_CI_URL=%q\n' "$1"
    printf 'LOCAL_CI_PORT=%q\n' "$port"
    [ -z "$kept" ] || printf '%s\n' "$kept"
  } >"$file.new"
  mv -f "$file.new" "$file"
}

cmd_install() {
  require_macos install
  local tool dns
  for tool in gh git just tailscale /usr/bin/python3; do
    command -v "$tool" >/dev/null || die "$tool not found"
  done
  gh auth status >/dev/null 2>&1 || die "gh is not logged in"
  dns=$(tailscale status --self --peers=false --json |
    /usr/bin/python3 -c 'import json, sys; print(json.load(sys.stdin)["Self"]["DNSName"].rstrip("."))') ||
    die "could not read this machine's tailnet name"
  mkdir -p "$out" "$state" "$LOCAL_CI_HOME/runner/scripts" "$HOME/Library/LaunchAgents"
  if [ ! -d "$checkout/.git" ]; then
    echo "cloning the CI checkout into $checkout"
    git clone --quiet "https://github.com/$repo.git" "$checkout"
  fi
  # Stop first: the running runner reads its copy of this script.
  cmd_stop >/dev/null
  cp scripts/local-ci.sh scripts/lib.sh "$LOCAL_CI_HOME/runner/scripts/"
  write_runner_env "https://$dns/ci"
  url=https://$dns/ci
  write_plists
  cmd_start
  tailscale serve --bg --set-path /ci "http://127.0.0.1:$port" >/dev/null
  echo "local CI installed: $context, checkout $checkout, results $url/"
  echo "check it with: just local-ci status"
}

cmd_uninstall() {
  require_macos uninstall
  cmd_stop
  rm -f "$(plist_path "$agent")" "$(plist_path "$web_agent")"
  tailscale serve --https=443 --set-path=/ci off >/dev/null 2>&1 || true
  echo "removed the agents and the /ci tailnet path; $LOCAL_CI_HOME is kept"
}

cmd_status() {
  local updated age
  if [ "$os" = macos ]; then
    local a
    for a in "$agent" "$web_agent"; do
      if launchctl print "gui/$(id -u)/$a" >/dev/null 2>&1; then
        echo "$a: $(launchctl print "gui/$(id -u)/$a" | awk '$1 == "state" { print $3; exit }')"
      else
        echo "$a: not loaded"
      fi
    done
  elif [ "$os" = windows ] && command -v schtasks >/dev/null; then
    local t
    for t in "mdoc local CI" "mdoc local CI web"; do
      echo "$t: $(schtasks //query //tn "$t" //fo list 2>/dev/null | sed -n 's/^Status: *//p' | grep . ||
        echo "not registered")"
    done
  fi
  if [ ! -f "$out/status.txt" ]; then
    echo "no status file at $out/status.txt: the runner has not run"
    return 1
  fi
  cat "$out/status.txt"
  [ -z "$url" ] || echo "results: $url/"
  updated=$(sed -n 's/^updated: //p' "$out/status.txt")
  age=$(($(date +%s) - $(file_epoch "$out/status.txt")))
  if [ "$age" -gt $((poll_s * 3)) ]; then
    echo "stale: last update $updated, ${age}s ago; the runner is not running"
    return 1
  fi
  if [ -f "$state/runner.log" ]; then
    echo "--- $state/runner.log"
    tail -n 10 "$state/runner.log"
  fi
}

file_epoch() {
  stat -c %Y "$1" 2>/dev/null || stat -f %m "$1"
}

main() {
  config
  case "${1:-run}" in
    run) cmd_run ;;
    once) cmd_run once ;;
    install) cmd_install ;;
    start) cmd_start ;;
    stop) cmd_stop ;;
    status) cmd_status ;;
    uninstall) cmd_uninstall ;;
    *) die "unknown command '$1' (run, once, install, start, stop, status, uninstall)" ;;
  esac
}

# The runner loop never returns to read more of this file, so replacing it
# while it runs is safe; exit before bash reads past this line anyway.
# scripts/test-local-ci.sh sources the functions with LOCAL_CI_SOURCED=1.
if [ -z "${LOCAL_CI_SOURCED:-}" ]; then
  main "$@"
  exit
fi
