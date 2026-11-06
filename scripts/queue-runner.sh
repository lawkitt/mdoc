#!/usr/bin/env bash
# Usage: QUEUE_CI_JOB=windows|android scripts/queue-runner.sh [run|once|status]
#
# The PC's runner for merge-queue commits (ADR 0095). One job per instance:
# QUEUE_CI_JOB=windows posts ci/windows (scripts/setup.sh, just lint, just
# test), QUEUE_CI_JOB=android posts ci/android (scripts/setup.sh, just
# android). The PC runs both side by side, each in its own home
# ($LOCAL_CI_HOME/queue-<job>, E:\ci\queue-<job>) with its own CI checkout,
# target/ and state. It tests only queue commits, the branches under
# gh-readonly-queue/main/ and ci-shadow/main/ (the shadow trial), never PR
# heads or main: polled every 30 s with this machine's gh login, oldest
# first, one at a time.
#
# scripts/merge-gate.sh reads its statuses (its header and pc_job):
#   - a commit outside the job's area: success "Not affected: no core or
#     <area> change", or "Docs only", from the diff, without a build;
#   - a group holding a fork or bot PR: nothing; merge-gate runs it hosted
#     (a PR that cannot be read is retried at the next poll, never taken
#     for third-party);
#   - a build: pending "Running <step> (<elapsed>)" at the start, at each
#     step and at least every 4 minutes (the heartbeat: 10 minutes without
#     one sends the job to hosted CI), then success "Passed in <time>",
#     failure "Failed in <step>" or "Timed out in <step>", or error for a
#     runner error or a commit the queue dropped while it ran. Each links its
#     log folder. A failure names the failing tests: nextest's from the log,
#     Gradle's from its JUnit XML (android:<class>#<method>).
# Whole-job limits: 30 minutes (windows), 40 (android); gh calls and fetches
# end after a minute, so a stalled one cannot hold the limit or the heartbeat
# up; setup and the checkout retry once, tests never. Upkeep: a job starts
# only with 40 GB free (a pending status says so once, so the heartbeat goes
# stale and merge-gate falls back), deletes target/ first when it is over
# 60 GB or rust-toolchain.toml changed, keeps headless renders only from a
# failed job, and output older than 7 days or past 20 GB goes, oldest first.
# Results are under $LOCAL_CI_HOME/out/queue-<job>/<sha>/ (log.txt;
# summary.txt with each step, the failing tests and, on a failure, the log's
# last 60 lines), which the local CI web task serves at
# $LOCAL_CI_URL/queue-<job>/; status.txt there has the runner's heartbeat, job
# and queue, an "offline:" line while it cannot read GitHub or origin (status
# then fails), and a "tailnet: /ci not served" line while `tailscale serve`
# does not list /ci. status checks that at once, and that status.txt answers
# at its tailnet URL, and fails when either does not.
#
#   run     the runner loop (what the scheduled task runs)
#   once    the loop until nothing is queued or running, then exit
#   status  the runner's status file and recent log
#
# Installed by scripts/queue-runner-setup.ps1 as a copy in
# $QUEUE_CI_HOME/runner/scripts/, which replaces itself with main's
# queue-runner.sh and lib.sh when idle and they differ. Settings are the
# QUEUE_CI_* variables in config(), also read from $QUEUE_CI_HOME/runner.env.
# Bash 3.2 compatible; scripts/test-queue-runner.sh checks it against fakes.
# The folder this copy runs from, before lib.sh moves to the repository root.
runner_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

every_area='core desktop recognition ios android ci'

config() {
  job=${QUEUE_CI_JOB:-}
  case "$job" in
    windows | android) ;;
    *) die "set QUEUE_CI_JOB to windows or android" ;;
  esac
  if [ -z "${LOCAL_CI_HOME:-}" ]; then
    if [ "$(platform)" = windows-x64 ]; then LOCAL_CI_HOME=/e/ci; else LOCAL_CI_HOME=$HOME/ci; fi
  fi
  home=${QUEUE_CI_HOME:-$LOCAL_CI_HOME/queue-$job}
  if [ -f "$home/runner.env" ]; then
    set -a
    # shellcheck source=/dev/null
    source "$home/runner.env"
    set +a
  fi
  context=ci/$job
  repo=${QUEUE_CI_REPO:-lawkitt/mdoc}
  prefixes=${QUEUE_CI_PREFIXES:-gh-readonly-queue/main/ ci-shadow/main/}
  checkout=$home/mdoc
  state=$home/state
  out=${QUEUE_CI_OUT:-$LOCAL_CI_HOME/out/queue-$job}
  url=${QUEUE_CI_URL:-}
  [ -n "$url" ] || [ -z "${LOCAL_CI_URL:-}" ] || url=$LOCAL_CI_URL/queue-$job
  poll_s=${QUEUE_CI_POLL:-30}
  tick_s=${QUEUE_CI_TICK:-5}
  # merge-gate takes a pending status 10 minutes old for a PC that is off.
  heartbeat_s=${QUEUE_CI_HEARTBEAT:-240}
  # The longest a gh call or fetch may take.
  net_s=${QUEUE_CI_NET_TIMEOUT:-60}
  # How long a failed checkout waits before its one retry.
  clean_wait_s=${QUEUE_CI_CLEAN_WAIT:-30}
  # The job's area (scripts/ci-areas.txt), steps and whole-job limit (ADR 0095).
  case "$job" in
    windows) area=desktop steps='setup lint test' limit_s=${QUEUE_CI_TIMEOUT:-1800} ;;
    android) area=android steps='setup android' limit_s=${QUEUE_CI_TIMEOUT:-2400} ;;
  esac
  min_free_gb=${QUEUE_CI_MIN_FREE_GB:-40}
  target_max_gb=${QUEUE_CI_TARGET_MAX_GB:-60}
  out_max_kb=$((${QUEUE_CI_OUT_MAX_GB:-20} * 1048576))
  keep_days=${QUEUE_CI_KEEP_DAYS:-7}
  # The tailnet page: `tailscale serve` must list /ci (the web task of
  # scripts/queue-runner-setup.ps1 sets it); status.txt checks every 10 min.
  tailscale_cmd=${QUEUE_CI_TAILSCALE:-tailscale}
  tailnet_s=600
}

log() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }

short() { printf '%s' "${1:0:7}"; }

duration() { printf '%dm%02ds' $(($1 / 60)) $(($1 % 60)); }

# How a step reads in logs and statuses.
label() {
  case "$1" in
    checkout) echo checkout ;;
    setup) echo scripts/setup.sh ;;
    *) echo "just $1" ;;
  esac
}

g() { git -C "$checkout" "$@"; }

# Runs a network command, ended after net_s: a stalled gh or git fetch would
# otherwise stop the heartbeat and the job's time limit with it.
bounded() { # command...
  local pid watch rc=0
  "$@" &
  pid=$!
  # Its own output closed, so a command substitution does not wait for it.
  (sleep "$net_s" && kill -TERM "$pid" 2>/dev/null) >/dev/null 2>&1 &
  watch=$!
  wait "$pid" || rc=$?
  kill "$watch" 2>/dev/null || true
  wait "$watch" 2>/dev/null || true
  return "$rc"
}

# A fetch from origin, bounded.
fetch() { bounded git -C "$checkout" fetch --quiet --no-tags --force origin "$@"; }

# --- GitHub -----------------------------------------------------------------

# The queue commits: `<branch> <sha>` lines. False when GitHub cannot be read.
list_queue() {
  local p
  for p in $prefixes; do
    bounded gh api --paginate "repos/$repo/git/matching-refs/heads/$p" \
      --jq '.[] | "\(.ref | sub("^refs/heads/"; "")) \(.object.sha)"' || return 1
  done
}

# `<author type> <cross-repository>` of PR $1. False when it cannot be read;
# its error is in $state/gh.err (a 404 when GitHub has no such PR).
pr_kind() {
  bounded gh api "repos/$repo/pulls/$1" --jq '"\(.user.type) \(.head.repo.full_name != .base.repo.full_name)"' 2>"$state/gh.err"
}

# Sets post_ok to whether GitHub took it.
post() { # sha state description [target url]
  local args=(api -X POST "repos/$repo/statuses/$1" -f "state=$2" -f "context=$context"
    -f "description=${3:0:140}")
  [ -z "${4:-}" ] || args+=(-f "target_url=$4")
  local try
  post_ok=true
  for try in 1 2 3; do
    if bounded gh "${args[@]}" >/dev/null; then
      log "$2 $context on $(short "$1"): $3"
      return 0
    fi
    [ "$try" -eq 3 ] || sleep "$tick_s"
  done
  post_ok=false
  log "warning: could not post $2 $context on $(short "$1"): $3"
}

# --- settling new commits -----------------------------------------------------

is_done() { grep -qxF "$1" "$state/done" 2>/dev/null; }

# A commit with a final status (or none to post) is never looked at again.
mark_done() {
  echo "$1" >>"$state/done"
  if [ "$(wc -l <"$state/done")" -gt 1000 ]; then
    tail -n 500 "$state/done" >"$state/done.new" && mv -f "$state/done.new" "$state/done"
  fi
}

# The areas a queue commit brings onto main: merge-base(main, sha)..sha, by
# the commit's own area table (every area when it has none).
commit_areas() { # sha merge-base
  local table
  if ! table=$(g show "$1:scripts/ci-areas.txt" 2>/dev/null); then
    echo "$every_area"
    return 0
  fi
  g diff --no-renames --name-only -z "$2" "$1" | changed_areas "$table"
}

# PR numbers from the commit subjects between $1 and $2: squash merges end
# "(#N)", merge commits say "Merge pull request #N" (as merge-gate reads them).
group_prs() { # merge-base sha
  g log --format=%s "$1..$2" | sed -nE 's/.*\(#([0-9]+)\)$/\1/p; s/^Merge pull request #([0-9]+).*/\1/p' | sort -u
}

# True when the group holds third-party code: a PR from a fork or by a bot,
# one GitHub does not know (404), or no PR number at all. Sets third_why.
# Exit 2 when a PR could not be read (GitHub down, a network or login
# error): undecided, so the commit is settled again at the next poll rather
# than left to hosted CI for good.
third_party() { # merge-base sha
  local prs pr line kind cross
  prs=$(group_prs "$1" "$2") || prs=''
  if [ -z "$prs" ]; then
    third_why="no PR number in its commit subjects"
    return 0
  fi
  for pr in $prs; do
    if ! line=$(pr_kind "$pr" | tr -d '\r'); then
      if ! grep -q 'HTTP 404' "$state/gh.err" 2>/dev/null; then
        third_why="could not read #$pr: $(tr -d '\r' <"$state/gh.err" 2>/dev/null | head -n 1)"
        return 2
      fi
      line='unknown true'
    fi
    read -r kind cross <<<"$line"
    if [ "$kind" != User ] || [ "$cross" != false ]; then
      third_why="#$pr (author ${kind:-unknown}, from a fork: ${cross:-unknown})"
      return 0
    fi
  done
  return 1
}

# Decides a new queue commit: a status from its diff, nothing for third-party
# code, or a place in the build queue.
settle() { # branch sha
  local branch=$1 sha=$2 mb areas text=''
  if ! mb=$(g merge-base refs/queue-ci/main "$sha"); then
    log "warning: $(short "$sha") shares no history with main; building it"
    mb=$sha
  fi
  if ! areas=$(commit_areas "$sha" "$mb"); then
    log "warning: $(short "$sha"): could not read its areas; building it"
    areas=$every_area
  fi
  if [ -z "$areas" ]; then
    text="Docs only"
  elif ! affects "$area" "$areas"; then
    text="Not affected: no core or $area change"
  fi
  if [ -n "$text" ]; then
    post "$sha" success "$text"
    ! $post_ok || mark_done "$sha"
    return 0
  fi
  local rc=0
  third_party "$mb" "$sha" || rc=$?
  case "$rc" in
    0)
      log "$(short "$sha") $branch: third-party code ($third_why); left to hosted CI"
      mark_done "$sha"
      return 0
      ;;
    2)
      log "warning: $(short "$sha") $branch: $third_why; retrying at the next poll"
      unsettled+="${unsettled:+, }$(short "$sha") ($third_why)"
      return 0
      ;;
  esac
  log "$(short "$sha") $branch: queued ($areas)"
  queued+="$(g log -1 --format=%ct "$sha") $(g rev-list --count "$sha") $sha $branch"$'\n'
}

# Sets listed to the queue's `<branch> <sha>` lines, settles new commits,
# and stops a job whose commit left the queue. False when GitHub or origin
# cannot be read.
poll() {
  local listing branch sha ref kept='' ct n
  if ! listing=$(list_queue | tr -d '\r'); then
    log "warning: could not list the queue; retrying at the next poll"
    unreachable "could not list the queue on GitHub"
    return 1
  fi
  if ! fetch "+refs/heads/main:refs/queue-ci/main"; then
    log "warning: could not fetch main; retrying at the next poll"
    unreachable "could not fetch main from origin"
    return 1
  fi
  [ -z "$offline_since" ] || log "GitHub reachable again after $(duration $(($(date +%s) - offline_since)))"
  offline_since='' offline_why='' unsettled=''
  listed=$listing
  polled=true
  [ -z "$run_pid" ] || is_listed "$run_sha" || stop_dropped
  # Commits that left the queue leave the build queue.
  while read -r ct n sha branch; do
    [ -z "$sha" ] || ! is_listed "$sha" || kept+="$ct $n $sha $branch"$'\n'
  done <<<"$queued"
  queued=$kept
  while read -r branch sha; do
    if [ -z "$sha" ] || [ "$sha" = "$run_sha" ]; then continue; fi
    ! is_done "$sha" || continue
    [[ $queued != *" $sha "* ]] || continue
    if ! fetch "+refs/heads/$branch:refs/queue-ci/c/$sha" ||
      [ "$(g rev-parse "refs/queue-ci/c/$sha")" != "$sha" ]; then
      log "warning: could not fetch $branch at $(short "$sha")"
      unsettled+="${unsettled:+, }$(short "$sha") (could not fetch it)"
      continue
    fi
    settle "$branch" "$sha"
  done <<<"$listing"
  # The refs of commits done or gone from the queue.
  for ref in $(g for-each-ref --format='%(refname)' refs/queue-ci/c/); do
    sha=${ref##*/}
    [ "$sha" = "$run_sha" ] || [[ $queued == *" $sha "* ]] || g update-ref -d "$ref" || true
  done
}

is_listed() { [[ $'\n'$listed$'\n' == *" $1"$'\n'* ]]; }

# A poll that could not read the queue: status.txt says so from the first
# failure on, so a fresh heartbeat with "queue: empty" never hides a runner
# that cannot see GitHub (the PC lost its route to GitHub for hours on
# 2026-11-04 while its status page looked idle).
unreachable() { # why
  [ -n "$offline_since" ] || offline_since=$(date +%s) offline_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  offline_why=$1
}

# --- the job ----------------------------------------------------------------

dir_kb() { du -sk "$1" 2>/dev/null | awk '{ print $1 }'; }

disk_free_gb() { df -Pk "$home" | awk 'NR == 2 { print int($4 / 1048576) }'; }

# Output folders older than keep_days, then the oldest while the output is
# over out_max_kb. Never while a job runs.
prune_out() {
  [ -d "$out" ] || return 0
  local d
  while IFS= read -r d; do
    d=${d##*/}
    log "deleting $out/$d: older than $keep_days days"
    rm -rf "${out:?}/${d:?}"
  done < <(find "$out" -mindepth 1 -maxdepth 1 -type d -mtime +$((keep_days - 1)))
  while [ "$(dir_kb "$out")" -gt "$out_max_kb" ]; do
    # shellcheck disable=SC2012 # the folders are short commit hashes
    d=$(cd "$out" && ls -1dtr -- */ 2>/dev/null | head -n 1)
    d=${d%/}
    [ -n "$d" ] || break
    log "deleting $out/$d: the output is over $((out_max_kb / 1048576)) GB"
    rm -rf "${out:?}/${d:?}"
  done
}

# Why a job deletes target/ first: the commit's rust-toolchain.toml differs
# from the last job's. Remembers this one's.
toolchain_reason() { # sha
  local now was
  now=$(g rev-parse --verify --quiet "$1:rust-toolchain.toml") || now=none
  was=$(cat "$state/toolchain" 2>/dev/null || true)
  echo "$now" >"$state/toolchain"
  [ -z "$was" ] || [ "$was" = "$now" ] || echo "rust-toolchain.toml changed"
}

# Standard input without colours, cursor moves or carriage returns at line ends.
plain() { sed -e "s/$(printf '\033')\[[0-9;?]*[A-Za-z]//g" -e "s/$(printf '\r')\$//"; }

# The failing tests nextest named in a log, one per line.
failed_tests() { # log
  plain <"$1" |
    sed -nE 's/^ *(FAIL|TIMEOUT|SIG[A-Z]+) +\[[^]]*\] +(\([0-9]+\/[0-9]+\) +)?//p' | sort -u
}

# The failing tests in Gradle's JUnit XML under the folders, as
# android:<class>#<method> (the form of .config/quarantine.txt). Gradle's log
# names none for emulator tests, only "There were failing tests" (#614). It
# writes <testcase> as `/>`, ` />` or `>`; only the last holds a <failure>.
gradle_failed_tests() { # folder...
  # A folder Gradle did not write is no error (pipefail would make it one).
  { find "$@" -type f -name 'TEST-*.xml' -exec sed -nE '/<testcase /{s/.*<testcase name="([^"]*)" classname="([^"]*)".*/android:\2#\1/;h;}
    /<(failure|error)[ >]/{x;p;}' {} + 2>/dev/null || true; } | sort -u
}

# The Android job's Gradle reports and JUnit XML, into the job's folder.
copy_android_outputs() { # dir
  local build=apps/android/app/build
  [ ! -d "$build/reports" ] || cp -R "$build/reports" "$1/android-reports" || true
  [ ! -d "$build/outputs/androidTest-results" ] || cp -R "$build/outputs/androidTest-results" "$1/androidTest-results" || true
  [ ! -d "$build/test-results" ] || cp -R "$build/test-results" "$1/test-results" || true
}

# The last lines of a failed job's log, for summary.txt.
log_excerpt() { # dir
  echo "--- log.txt, last 60 lines"
  { tail -n 60 "$1/log.txt" 2>/dev/null || true; } | plain
}

# The commit checked out with no untracked files but target/.
checkout_clean() { # sha
  git checkout --quiet --force --detach "$1" || return 1
  # Keep the build cache. Git Bash rewrites a native program's argument that
  # starts with / as a Windows path, so /target/ would match nothing.
  MSYS_NO_PATHCONV=1 git clean -ffdxq -e /target/
}

# Runs in the background in its own process group; output goes to log.txt.
# Exit 0 passed, 1 a step failed (named in .step), 3 the checkout failed.
job() { # sha dir clean-reason
  set +m
  local sha=$1 dir=$2 why=$3 step start rc kb
  cd "$checkout" || exit 3
  unset CARGO_TARGET_DIR RUSTFLAGS MDOC_HEADLESS_ARTIFACTS
  printf 'checkout %s\n' "$(date +%s)" >"$dir/.step"
  if [ -z "$why" ] && [ -d target ]; then
    kb=$(dir_kb target)
    [ "${kb:-0}" -le $((target_max_gb * 1048576)) ] || why="target/ is $((kb / 1048576)) GB, over $target_max_gb GB"
  fi
  if [ -n "$why" ]; then
    echo "queue-runner: deleting target/: $why"
    echo "upkeep: deleted target/ ($why)" >>"$dir/summary.txt"
    rm -rf "${checkout:?}/target" || exit 3
  fi
  echo "queue-runner: checkout $sha"
  if ! checkout_clean "$sha"; then
    # A stopped job's processes (Gradle, its test run) can hold files for a
    # while after they are killed; on Windows git then cannot remove them
    # (2026-11-04, the job after a stopped Android job). Infrastructure: one
    # retry, logged (ADR 0095).
    echo "queue-runner: checkout failed; retrying once in ${clean_wait_s}s"
    echo "checkout: failed once; retried" >>"$dir/summary.txt"
    sleep "$clean_wait_s"
    checkout_clean "$sha" || exit 3
  fi
  for step in $steps; do
    start=$(date +%s)
    printf '%s %s\n' "$step" "$start" >"$dir/.step"
    echo "queue-runner: $(label "$step")"
    rc=0
    case "$step" in
      # Not through just: setup may replace the just binary. Its downloads
      # are infrastructure: one retry, logged (ADR 0095); tests never retry.
      setup)
        if ! bash scripts/setup.sh </dev/null; then
          echo "queue-runner: scripts/setup.sh failed; retrying once"
          echo "scripts/setup.sh: failed once; retried" >>"$dir/summary.txt"
          bash scripts/setup.sh </dev/null || rc=$?
        fi
        ;;
      test) MDOC_HEADLESS_ARTIFACTS=$dir/renders just test || rc=$? ;;
      android) MDOC_ANDROID_JOB=always just android || rc=$? ;;
      *) just "$step" || rc=$? ;;
    esac
    # The Android job's Gradle reports and test results, passed or failed.
    [ "$step" != android ] || copy_android_outputs "$dir"
    if [ "$rc" -eq 0 ]; then
      echo "$(label "$step"): passed in $(duration $(($(date +%s) - start)))" >>"$dir/summary.txt"
    else
      echo "$(label "$step"): failed (exit $rc) after $(duration $(($(date +%s) - start)))" >>"$dir/summary.txt"
      exit 1
    fi
  done
  # Headless renders are about 1.1 GB a Windows run: kept only from a failed
  # one, so the output's 20 GB keep about 7 days of logs (#614).
  if [ -d "$dir/renders" ]; then
    rm -rf "${dir:?}/renders"
    echo "upkeep: deleted renders/ (the job passed)" >>"$dir/summary.txt"
  fi
  exit 0
}

start_job() { # branch sha
  local branch=$1 sha=$2 name why
  name=$(short "$sha")
  why=$(toolchain_reason "$sha")
  rm -rf "${out:?}/${name:?}"
  mkdir -p "$out/$name"
  run_sha=$sha run_branch=$branch run_dir=$out/$name run_start=$(date +%s) run_step=checkout run_beat=0
  run_url=''
  [ -z "$url" ] || run_url=$url/$name/
  cat >"$run_dir/summary.txt" <<EOF
$context: https://github.com/$repo/commit/$sha
branch: $branch
started: $(date -u +%Y-%m-%dT%H:%M:%SZ)
steps: $steps
EOF
  post "$sha" pending "Running $(label checkout) (0m00s)" "$run_url"
  ! $post_ok || run_beat=$run_start
  set -m
  job "$sha" "$run_dir" "$why" </dev/null >"$run_dir/log.txt" 2>&1 &
  run_pid=$!
  set +m
  echo "$run_pid" >"$state/job.pid"
  log "started $(short "$sha") $branch (pid $run_pid)"
}

# Stops the job's whole process group (cargo, rustc, test binaries).
kill_job() {
  local pid=$1 w
  if [ "$(platform)" = windows-x64 ] && command -v taskkill >/dev/null; then
    # Git Bash: native children (cargo.exe, rustc.exe) are not in the MSYS
    # process group, so end each group member's Windows process tree. The
    # job's own shell goes first: a step whose child dies can return 0, and
    # the job would start its next step before its turn came.
    local members
    members=$(ps | awk -v g="$pid" 'NR > 1 { if ($1 !~ /^[0-9]+$/) $1 = ""; $0 = $0; if ($3 == g) print $4 }')
    kill -KILL "$pid" 2>/dev/null || true
    for w in $members; do
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
  post "$run_sha" "$1" "$2" "$run_url"
  echo "result: $1, $2" >>"$run_dir/summary.txt"
  # A failed or timed-out step: where it ended, without opening log.txt.
  [ "$1" != failure ] || log_excerpt "$run_dir" >>"$run_dir/summary.txt"
  rm -f "$state/job.pid" "$run_dir/.step"
  # Never run again, even when GitHub did not take the result: the heartbeat
  # stops, so merge-gate runs it hosted.
  mark_done "$run_sha"
  run_pid='' run_sha='' run_branch='' run_step=''
  prune_out
  next_poll=0
}

check_job() {
  local step now rc=0 elapsed tests first more
  now=$(date +%s)
  elapsed=$((now - run_start))
  if kill -0 "$run_pid" 2>/dev/null; then
    read -r step _ 2>/dev/null <"$run_dir/.step" || step=$run_step
    if [ "$elapsed" -gt "$limit_s" ]; then
      log "$(short "$run_sha"): the job passed its $((limit_s / 60)) min limit in $(label "$step")"
      echo "queue-runner: timed out after $(duration "$limit_s") in $(label "$step")" >>"$run_dir/log.txt"
      kill_job "$run_pid"
      finish failure "Timed out in $(label "$step") after $(duration "$limit_s")"
      return 0
    fi
    if [ "$step" != "$run_step" ] || [ $((now - run_beat)) -ge "$heartbeat_s" ]; then
      run_step=$step
      post "$run_sha" pending "Running $(label "$step") ($(duration "$elapsed"))" "$run_url"
      ! $post_ok || run_beat=$now
    fi
    return 0
  fi
  wait "$run_pid" || rc=$?
  # Ended past the limit while this loop was held up: it still timed out.
  if [ "$elapsed" -gt "$limit_s" ]; then
    read -r step _ 2>/dev/null <"$run_dir/.step" || step=$run_step
    echo "queue-runner: ended after $(duration "$elapsed"), past its $(duration "$limit_s") limit" >>"$run_dir/log.txt"
    finish failure "Timed out in $(label "$step") after $(duration "$limit_s")"
    return 0
  fi
  case "$rc" in
    0) finish success "Passed in $(duration "$elapsed")" ;;
    1)
      read -r step _ 2>/dev/null <"$run_dir/.step" || step='?'
      if [ "$step" = android ]; then
        tests=$(gradle_failed_tests "$run_dir/androidTest-results" "$run_dir/test-results")
      else
        tests=$(failed_tests "$run_dir/log.txt")
      fi
      if [ -n "$tests" ]; then
        printf '%s\n' "$tests" >"$run_dir/failed-tests.txt"
        printf 'failed: %s\n' "$tests" >>"$run_dir/summary.txt"
        first=$(head -n 1 <<<"$tests")
        more=$(($(wc -l <<<"$tests") - 1))
        [ "$more" -eq 0 ] || first+=" and $more more"
        finish failure "Failed in $(label "$step"): $first"
      else
        finish failure "Failed in $(label "$step")"
      fi
      ;;
    3) finish error "Runner error: could not check out the commit; see the log" ;;
    *) finish error "Runner error (exit $rc); see the log" ;;
  esac
}

# The queue dropped the running commit's group, or merged it on hosted CI's
# result first: either way this job's result no longer counts.
stop_dropped() {
  local why="the queue dropped this commit"
  ! g merge-base --is-ancestor "$run_sha" refs/queue-ci/main 2>/dev/null || why="merged before this job finished"
  log "$(short "$run_sha") $run_branch: left the queue ($why); stopping its job"
  echo "queue-runner: stopped: $why" >>"$run_dir/log.txt"
  kill_job "$run_pid"
  finish error "Stopped: $why"
}

# The build queue, oldest commit first: by commit time, then by history
# length, since a queue entry's commit builds on the one before it.
queue_sorted() { awk 'NF' <<<"$queued" | sort -n -k1,1 -k2,2; }

# Starts the oldest queued commit. Below the disk floor it says so once per
# commit: a pending status that is not refreshed, so after 10 minutes
# merge-gate counts the PC as off and runs the job hosted.
start_next() {
  [ -n "$queued" ] || return 1
  local ct n sha branch free
  read -r ct n sha branch <<<"$(queue_sorted | head -n 1)"
  free=$(disk_free_gb)
  if [ "${free:-0}" -lt "$min_free_gb" ]; then
    waiting="${free:-?} GB free; a job starts at $min_free_gb GB"
    if [ "$disk_noted" != "$sha" ]; then
      log "waiting: $waiting"
      post "$sha" pending "Waiting for disk space: $waiting" "${url:+$url/status.txt}"
      disk_noted=$sha
    fi
    return 1
  fi
  waiting=''
  queued=$(grep -vF " $sha " <<<"$queued" || true)
  [ -z "$queued" ] || queued+=$'\n'
  start_job "$branch" "$sha"
}

# --- self-update ----------------------------------------------------------------

# The files of this copy that differ from main's, line endings aside.
outdated_files() {
  local f list=''
  for f in queue-runner.sh lib.sh; do
    g cat-file -e "refs/queue-ci/main:scripts/$f" 2>/dev/null || continue
    cmp -s <(g show "refs/queue-ci/main:scripts/$f" | tr -d '\r') \
      <(tr -d '\r' <"$runner_dir/$f" 2>/dev/null) || list+="${list:+ }$f"
  done
  echo "$list"
}

# The installed copy, idle, takes main's scripts and restarts as the new
# copy. A copy run from elsewhere (a checkout) never rewrites itself.
self_update() { # mode
  local installed files f main
  installed=$(cd "$home/runner/scripts" 2>/dev/null && pwd -P) || return 0
  [ "$runner_dir" = "$installed" ] || return 0
  files=$(outdated_files)
  [ -n "$files" ] || return 0
  main=$(g rev-parse refs/queue-ci/main)
  [ "$main" != "${update_failed:-}" ] || return 0
  for f in queue-runner.sh lib.sh; do
    g cat-file -e "refs/queue-ci/main:scripts/$f" 2>/dev/null || continue
    # A failed write leaves the copy outdated, which the check below logs.
    if g show "refs/queue-ci/main:scripts/$f" >"$runner_dir/$f.new"; then
      mv -f "$runner_dir/$f.new" "$runner_dir/$f" || true
    fi
  done
  if [ -n "$(outdated_files)" ]; then
    log "warning: could not update this runner's $files from main@$(short "$main")"
    update_failed=$main
    return 0
  fi
  log "updated this runner's $files from main@$(short "$main"); restarting"
  rm -rf "$state/lock"
  trap - EXIT
  exec bash "$runner_dir/queue-runner.sh" "$1"
}

# --- the loop -------------------------------------------------------------------

# Whether `tailscale serve` lists /ci, the way to the results from the
# tailnet: 0 yes, 1 no (or it cannot say), 2 no tailscale here (macOS and
# Linux test runs). /ci went once while status.txt stayed fresh (#614).
ci_served() {
  command -v "$tailscale_cmd" >/dev/null 2>&1 || return 2
  # Its JSON names each served path, as "/ci": {...}.
  bounded "$tailscale_cmd" serve status --json 2>/dev/null | grep -q '"/ci/\{0,1\}" *:'
}

# True when this runner's status.txt answers at its URL on the tailnet.
page_answers() {
  { bounded curl -fsS --max-time "$net_s" "$url/status.txt" 2>/dev/null || true; } |
    tr -d '\r' | grep -q "^$context queue runner on "
}

# Sets tailnet to the status.txt line about /ci, checked at most every
# tailnet_s seconds.
check_tailnet() {
  local now rc=0
  now=$(date +%s)
  [ $((now - tailnet_at)) -ge "$tailnet_s" ] || return 0
  tailnet_at=$now
  ci_served || rc=$?
  tailnet=''
  [ "$rc" -ne 1 ] || tailnet="tailnet: /ci not served"
}

write_status() {
  local next=$out/.status.txt q
  q=$(queue_sorted | awk '{ printf "%s%s %s", (NR > 1 ? ", " : ""), substr($3, 1, 7), $4 }')
  check_tailnet
  {
    echo "$context queue runner on $(hostname)"
    echo "heartbeat: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    if [ -n "$run_pid" ]; then
      echo "running: $(short "$run_sha") $run_branch, $(label "$run_step") for $(duration $(($(date +%s) - run_start))) ${run_url:-$run_dir}"
    else
      echo "running: nothing"
    fi
    if [ -n "$offline_since" ]; then
      echo "queue: unknown (${q:-nothing known})"
      echo "offline: $offline_why since $offline_at; no status can be posted"
    else
      echo "queue: ${q:-empty}"
    fi
    [ -z "$waiting" ] || echo "waiting: $waiting"
    [ -z "$unsettled" ] || echo "retrying: $unsettled"
    [ -z "${update_failed:-}" ] || echo "outdated: could not update from main@$(short "$update_failed")"
    [ -z "$tailnet" ] || echo "$tailnet"
  } >"$next" && mv -f "$next" "$out/status.txt"
}

# True when $1 is a live runner or job, not a pid reused after a restart.
is_runner() {
  if [ -z "$1" ] || ! kill -0 "$1" 2>/dev/null; then return 1; fi
  if [ "$(platform)" = windows-x64 ]; then
    ps -p "$1" 2>/dev/null | grep -q '/bash'
  else
    ps -p "$1" -o args= 2>/dev/null | grep -q 'queue-runner\.sh'
  fi
}

lock() {
  local pid
  if ! mkdir "$state/lock" 2>/dev/null; then
    pid=$(cat "$state/lock/pid" 2>/dev/null || true)
    ! is_runner "$pid" || die "a $context runner is already running (pid $pid)"
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

# shellcheck disable=SC2317,SC2329 # the EXIT trap
on_exit() {
  if [ -n "${run_pid:-}" ]; then
    log "stopping; $(short "$run_sha") runs again when the runner starts"
    kill_job "$run_pid"
    rm -f "$state/job.pid"
  fi
  rm -rf "$state/lock"
}

cmd_run() { # run|once
  local mode=$1 now
  [ -d "$checkout/.git" ] || die "no CI checkout at $checkout; install with scripts/queue-runner-setup.ps1"
  mkdir -p "$out" "$state"
  lock
  if [ "$mode" = run ] && [ -z "${QUEUE_CI_FOREGROUND:-}" ]; then
    echo "$context queue runner: logging to $state/runner.log"
    exec >>"$state/runner.log" 2>&1
  fi
  trap on_exit EXIT
  trap 'exit 0' TERM INT HUP
  log "runner started: $context for $repo ($prefixes), checkout $checkout, copy in $runner_dir"
  kill_stale_job
  queued='' listed='' polled=false run_pid='' run_sha='' run_branch='' run_step='' run_dir='' run_url=''
  waiting='' disk_noted='' next_poll=0 post_ok=true update_failed='' offline_since='' offline_at='' offline_why='' unsettled=''
  tailnet='' tailnet_at=0
  prune_out
  while :; do
    [ -z "$run_pid" ] || check_job
    now=$(date +%s)
    if [ "$now" -ge "$next_poll" ]; then
      next_poll=$((now + poll_s))
      if poll && [ -z "$run_pid" ]; then
        self_update "$mode"
        start_next || true
      fi
    fi
    write_status
    # Not while a commit waits to be settled again or GitHub cannot be read:
    # the queue is not known to be empty.
    if [ "$mode" = once ] && $polled && [ -z "$run_pid" ] && [ "$next_poll" != 0 ] && [ -z "$queued" ] &&
      [ -z "$unsettled" ] && [ -z "$offline_since" ]; then
      log "queue empty"
      return 0
    fi
    sleep "$tick_s"
  done
}

file_epoch() { stat -c %Y "$1" 2>/dev/null || stat -f %m "$1"; }

cmd_status() {
  local age served=0 rc=0
  if [ "$(platform)" = windows-x64 ] && command -v schtasks >/dev/null; then
    echo "mdoc queue CI $job: $(schtasks //query //tn "mdoc queue CI $job" //fo list 2>/dev/null |
      sed -n 's/^Status: *//p' | grep . || echo "not registered")"
  fi
  if [ ! -f "$out/status.txt" ]; then
    echo "no status file at $out/status.txt: the runner has not run"
    return 1
  fi
  # Read now, not from status.txt's 10-minute check.
  grep -v '^tailnet: ' "$out/status.txt"
  [ -z "$url" ] || echo "results: $url/"
  ci_served || served=$?
  case "$served" in
    0)
      # The page itself (ADR 0100): served is not enough if the web server
      # is down or serves another folder.
      if [ -z "$url" ]; then
        echo "tailnet: /ci served; no results URL (QUEUE_CI_URL) to fetch"
      elif page_answers; then
        echo "tailnet: /ci served, and $url/status.txt answers"
      else
        echo "unhealthy: tailscale serves /ci, but $url/status.txt does not answer with this runner's status;" \
          "check the 'mdoc queue CI web' task"
        rc=1
      fi
      ;;
    2) echo "tailnet: no $tailscale_cmd here; /ci not checked" ;;
    *)
      echo "unhealthy: tailscale serve does not list /ci, so the results are not on the tailnet;" \
        "restart the 'mdoc queue CI web' task, or run: tailscale serve --bg --set-path /ci http://127.0.0.1:8787"
      rc=1
      ;;
  esac
  if grep -q '^offline: ' "$out/status.txt"; then
    echo "unhealthy: the runner cannot read GitHub"
    return 1
  fi
  age=$(($(date +%s) - $(file_epoch "$out/status.txt")))
  if [ "$age" -gt $((poll_s * 3)) ]; then
    echo "stale: last heartbeat ${age}s ago; the runner is not running"
    return 1
  fi
  if [ -f "$state/runner.log" ]; then
    echo "--- $state/runner.log"
    tail -n 10 "$state/runner.log"
  fi
  return "$rc"
}

main() {
  config
  case "${1:-run}" in
    run | once) cmd_run "${1:-run}" ;;
    status) cmd_status ;;
    *) die "unknown command '$1' (run, once, status)" ;;
  esac
}

# The loop never returns to read more of this file, so replacing it while
# it runs is safe; exit before bash reads past this line anyway.
# scripts/test-queue-runner.sh sources the functions with QUEUE_RUNNER_SOURCED=1.
if [ -z "${QUEUE_RUNNER_SOURCED:-}" ]; then
  main "$@"
  exit
fi
