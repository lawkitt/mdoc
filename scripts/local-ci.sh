#!/usr/bin/env bash
# Usage: scripts/local-ci.sh [run|once|install|start|stop|status|uninstall]
#
# Local CI (ADR 0082). Tests each open non-draft PR from a branch in the
# repository (never forks or bots) at refs/pull/N/merge, in a CI checkout that
# keeps its target/, and posts the local/macos or local/windows commit status
# on the PR head. Polls every 60 s with this machine's gh login; one job at a
# time; a newer head replaces its PR's queued or running job. Logs, summaries
# and renders stay in $LOCAL_CI_HOME/out for 7 days, served to the tailnet.
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
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

agent=com.lawkitt.mdoc.local-ci
web_agent=com.lawkitt.mdoc.local-ci-web

config() {
  local default_os=
  case "$(platform)" in
    macos-arm64) default_os=macos ;;
    windows-x64) default_os=windows ;;
  esac
  if [ -z "${LOCAL_CI_HOME:-}" ]; then
    case "$default_os" in
      windows) LOCAL_CI_HOME=/d/ci ;;
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
  context=local/$os
  repo=${LOCAL_CI_REPO:-lawkitt/mdoc}
  checkout=$LOCAL_CI_HOME/mdoc
  out=$LOCAL_CI_HOME/out
  state=$LOCAL_CI_HOME/state
  url=${LOCAL_CI_URL:-}
  port=${LOCAL_CI_PORT:-8787}
  poll_s=${LOCAL_CI_POLL:-60}
  tick_s=${LOCAL_CI_TICK:-5}
  steps=${LOCAL_CI_STEPS:-lint test}
  # The hosted job limits (ADR 0082): clippy 20 min, test 30 min.
  lint_s=${LOCAL_CI_LINT_TIMEOUT:-1200}
  test_s=${LOCAL_CI_TEST_TIMEOUT:-1800}
  keep_days=${LOCAL_CI_KEEP_DAYS:-7}
  # The Mac is a build slot beside two agent builders (ADR 0082).
  min_free_gb=${LOCAL_CI_MIN_FREE_GB:-40}
  max_swap_mb=${LOCAL_CI_MAX_SWAP_MB:-2048}
}

log() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }

short() { printf '%s' "${1:0:7}"; }

duration() { printf '%dm%02ds' $(($1 / 60)) $(($1 % 60)); }

# --- paths-ignore: ci.yml's list is the one place both read (ADR 0082) -----

# The first paths-ignore list in the ci.yml on stdin, one pattern per line.
paths_ignore() {
  awk '
    done { next }
    inlist {
      if ($0 ~ /^[[:space:]]*-[[:space:]]/) {
        sub(/^[[:space:]]*-[[:space:]]*/, "")
        sub(/[[:space:]]+#.*$/, "")
        gsub(/^["'\'']|["'\''][[:space:]]*$/, "")
        print
        next
      }
      done = 1
      next
    }
    /^[[:space:]]*paths-ignore:/ { inlist = 1 }
  '
}

# A GitHub filter glob as an anchored extended regex: ** crosses directories,
# * and ? do not.
glob_regex() {
  local glob=$1 re='' c i=0
  while [ "$i" -lt "${#glob}" ]; do
    c=${glob:i:1}
    if [ "${glob:i:3}" = '**/' ]; then
      re+='(.*/)?'
      i=$((i + 3))
      continue
    elif [ "${glob:i:2}" = '**' ]; then
      re+='.*'
      i=$((i + 2))
      continue
    fi
    case "$c" in
      '*') re+='[^/]*' ;;
      '?') re+='[^/]' ;;
      . | \\ | + | '(' | ')' | '|' | ^ | '$' | '{' | '}' | '[' | ']') re+="\\$c" ;;
      *) re+=$c ;;
    esac
    i=$((i + 1))
  done
  printf '^%s$' "$re"
}

# True when every file on stdin matches one of the globs given as arguments;
# false for an empty list.
only_ignored() {
  local regexes=() glob f any=false r matched
  for glob in "$@"; do regexes+=("$(glob_regex "$glob")"); done
  [ "${#regexes[@]}" -gt 0 ] || return 1
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    any=true
    matched=false
    for r in "${regexes[@]}"; do
      if [[ $f =~ $r ]]; then
        matched=true
        break
      fi
    done
    $matched || return 1
  done
  $any
}

# True when the PR in merge commit $1 touches only ci.yml's paths-ignore files.
docs_only() {
  local yml files globs=()
  yml=$(git -C "$checkout" show "$1:.github/workflows/ci.yml" 2>/dev/null) || return 1
  while IFS= read -r g; do [ -z "$g" ] || globs+=("$g"); done < <(paths_ignore <<<"$yml")
  [ "${#globs[@]}" -gt 0 ] || return 1
  files=$(git -C "$checkout" diff --no-renames --name-only "$1^1...$1^2") || return 1
  only_ignored "${globs[@]}" <<<"$files"
}

# --- GitHub -----------------------------------------------------------------

# One line per open PR: number, draft, cross-repository, author type,
# mergeable, base, head, this runner's status state on the head (or NONE) and
# when it was posted.
# shellcheck disable=SC2016 # GraphQL variables, not shell ones
poll_query='query($owner: String!, $name: String!, $context: String!) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 50) {
      nodes {
        number isDraft isCrossRepository mergeable baseRefName headRefOid
        author { __typename }
        commits(last: 1) { nodes { commit { status { context(name: $context) { state createdAt } } } } }
      }
    }
  }
}'
poll_jq='.data.repository.pullRequests.nodes[] | [.number, .isDraft, .isCrossRepository,
  (.author.__typename // "None"), .mergeable, .baseRefName, .headRefOid,
  (.commits.nodes[0].commit.status.context.state // "NONE"),
  (.commits.nodes[0].commit.status.context.createdAt // "")] | @tsv'

# Sets prs to the PRs this runner tests: not drafts, from branches in the
# repository, opened by a user (Dependabot is a bot).
poll() {
  local lines
  if ! lines=$(gh api graphql -f owner="${repo%/*}" -f name="${repo#*/}" \
    -f context="$context" -f query="$poll_query" --jq "$poll_jq"); then
    log "warning: could not list PRs; retrying at the next poll"
    return 1
  fi
  prs=$(awk -F'\t' '$2 == "false" && $3 == "false" && $4 == "User"' <<<"$lines")
  polled=true
}

post() { # sha state description [target url]
  local args=(api -X POST "repos/$repo/statuses/$1" -f "state=$2" -f "context=$context"
    -f "description=${3:0:140}")
  [ -z "${4:-}" ] || args+=(-f "target_url=$4")
  if gh "${args[@]}" >/dev/null; then
    log "$2 on $(short "$1"): $3"
  else
    log "warning: could not post $2 on $(short "$1"): $3"
  fi
}

pr_line() { awk -F'\t' -v n="$1" '$1 == n' <<<"$prs"; }

drop_pr() { prs=$(awk -F'\t' -v n="$1" '$1 != n' <<<"$prs"); }

# PRs waiting for this runner, oldest status first: no status yet (posted
# "Queued" by queue_new) or pending (queued, or left by a stopped runner).
queue() {
  awk -F'\t' -v run="${run_pr:-}" '
    $1 != run && ($8 == "NONE" || $8 == "PENDING")' <<<"$prs" | sort -t$'\t' -k9,9 -k1,1n
}

queue_new() {
  local now n head
  now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  while IFS=$'\t' read -r n _ _ _ _ _ head _ _; do
    post "$head" pending "Queued" "${url:+$url/status.txt}"
    prs=$(awk -F'\t' -v OFS='\t' -v n="$n" -v t="$now" \
      '$1 == n { $8 = "PENDING"; $9 = t } { print }' <<<"$prs")
  done < <(awk -F'\t' -v run="${run_pr:-}" '$1 != run && $8 == "NONE"' <<<"$prs")
}

# --- the job ----------------------------------------------------------------

capacity_ok() {
  waiting=
  [ "$os" = macos ] || return 0
  local swap_mb free_gb
  swap_mb=$(sysctl -n vm.swapusage | awk '{
    for (i = 1; i < NF; i++) if ($i == "used") {
      v = $(i + 2); u = substr(v, length(v)); v = substr(v, 1, length(v) - 1)
      print int(u == "G" ? v * 1024 : v); exit
    } }')
  free_gb=$(df -Pk "$LOCAL_CI_HOME" | awk 'NR == 2 { print int($4 / 1048576) }')
  if [ "${swap_mb:-0}" -ge "$max_swap_mb" ]; then
    waiting="swap ${swap_mb} MB in use (starts under $max_swap_mb MB)"
  elif [ "${free_gb:-0}" -lt "$min_free_gb" ]; then
    waiting="${free_gb} GB free (starts at $min_free_gb GB)"
  fi
  [ -z "$waiting" ]
}

timeout_for() {
  case "$1" in
    lint) echo "$lint_s" ;;
    *) echo "$test_s" ;;
  esac
}

# Runs in the background in its own process group; output goes to log.txt.
# Exit 0 passed, 1 a step failed (named in .step), 3 the checkout failed.
job() { # merge-ref dir
  set +m
  local dir=$2 step start rc
  cd "$checkout" || exit 3
  unset CARGO_TARGET_DIR RUSTFLAGS
  echo "local-ci: checkout $(git rev-parse "$1")"
  git checkout --quiet --force --detach "$1" || exit 3
  git clean -ffdxq -e /target/ || exit 3
  for step in $steps; do
    start=$(date +%s)
    printf '%s %s\n' "$step" "$start" >"$dir/.step"
    echo "local-ci: just $step"
    rc=0
    if [ "$step" = test ]; then
      MDOC_HEADLESS_ARTIFACTS=$dir/renders just "$step" || rc=$?
    else
      just "$step" || rc=$?
    fi
    if [ "$rc" -eq 0 ]; then
      echo "just $step: passed in $(duration $(($(date +%s) - start)))" >>"$dir/summary.txt"
    else
      echo "just $step: failed (exit $rc) after $(duration $(($(date +%s) - start)))" >>"$dir/summary.txt"
      exit 1
    fi
  done
  exit 0
}

start_job() { # pr head base base-sha
  local dir
  dir=$out/pr-$1/$(short "$2")
  rm -rf "$dir"
  mkdir -p "$dir"
  run_pr=$1 run_head=$2 run_base=$3 run_base_sha=$4 run_dir=$dir run_start=$(date +%s)
  run_url=
  [ -z "$url" ] || run_url=$url/pr-$1/$(short "$2")/
  cat >"$dir/summary.txt" <<EOF
$context: https://github.com/$repo/pull/$1
head: $2
base: $3 $4
started: $(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF
  post "$2" pending "Running on $3@$(short "$4")" "$run_url"
  set -m
  job refs/local-ci/merge "$dir" </dev/null >"$dir/log.txt" 2>&1 &
  run_pid=$!
  set +m
  echo "$run_pid" >"$state/job.pid"
  log "started #$1 $(short "$2") on $3@$(short "$4") (pid $run_pid)"
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
  post "$run_head" "$1" "$2" "$run_url"
  echo "result: $1, $2" >>"$run_dir/summary.txt"
  rm -f "$state/job.pid" "$run_dir/.step"
  run_pr='' run_head='' run_pid=''
  # Statuses changed: read them again before starting the next job.
  prs=
  next_poll=0
}

check_job() {
  local step since limit rc=0 on
  on="$run_base@$(short "$run_base_sha")"
  if kill -0 "$run_pid" 2>/dev/null; then
    read -r step since 2>/dev/null <"$run_dir/.step" || return 0
    limit=$(timeout_for "$step")
    if [ $(($(date +%s) - since)) -gt "$limit" ]; then
      log "#$run_pr: just $step passed its $((limit / 60)) min limit"
      echo "local-ci: just $step timed out after $(duration "$limit")" >>"$run_dir/log.txt"
      kill_job "$run_pid"
      finish failure "Timed out in just $step after $(duration "$limit") on $on"
    fi
    return 0
  fi
  wait "$run_pid" || rc=$?
  case "$rc" in
    0) finish success "Passed on $on in $(duration $(($(date +%s) - run_start)))" ;;
    1)
      read -r step _ 2>/dev/null <"$run_dir/.step" || step='?'
      finish failure "Failed in just $step on $on"
      ;;
    *) finish error "Runner error (exit $rc) on $on; see the log" ;;
  esac
}

# A newer head replaces a running job; a closed or drafted PR ends it.
sync_running() {
  [ -n "${run_pr:-}" ] || return 0
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

# Starts the first startable queued PR; settles conflicts and docs-only PRs
# without a build. Returns 0 if a job started.
start_next() {
  local n mergeable base head merge base_sha
  capacity_ok || return 1
  while IFS=$'\t' read -r n _ _ _ mergeable base head _ _; do
    case "$mergeable" in
      CONFLICTING)
        post "$head" failure "Conflicts with $base"
        drop_pr "$n"
        continue
        ;;
      MERGEABLE) ;;
      *) continue ;; # GitHub is still computing the merge
    esac
    if ! git -C "$checkout" fetch --quiet --no-tags --force origin \
      "+refs/pull/$n/merge:refs/local-ci/merge"; then
      log "warning: could not fetch refs/pull/$n/merge"
      continue
    fi
    merge=$(git -C "$checkout" rev-parse refs/local-ci/merge)
    if [ "$(git -C "$checkout" rev-parse --verify --quiet "$merge^2")" != "$head" ]; then
      log "#$n: merge ref not yet at $(short "$head")"
      continue
    fi
    base_sha=$(git -C "$checkout" rev-parse "$merge^1")
    if docs_only "$merge"; then
      post "$head" success "Docs only, not tested ($base@$(short "$base_sha"))"
      drop_pr "$n"
      continue
    fi
    start_job "$n" "$head" "$base" "$base_sha"
    return 0
  done < <(queue)
  return 1
}

prune() {
  [ -d "$out" ] || return 0
  find "$out" -mindepth 2 -maxdepth 2 -type d -mtime +$((keep_days - 1)) -exec rm -rf {} + 2>/dev/null || true
  find "$out" -mindepth 1 -maxdepth 1 -type d -empty -exec rmdir {} + 2>/dev/null || true
}

write_status() {
  local next=$out/.status.txt q
  q=$(queue | awk -F'\t' '{ printf "%s#%s %s", (NR > 1 ? ", " : ""), $1, substr($7, 1, 7) }')
  {
    echo "$context runner on $(hostname)"
    echo "updated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    if [ -n "${run_pr:-}" ]; then
      local step='?' since
      read -r step since 2>/dev/null <"$run_dir/.step" || since=$run_start
      echo "running: #$run_pr $(short "$run_head") on $run_base@$(short "$run_base_sha"), just $step for $(duration $(($(date +%s) - since))) ${run_url:-$run_dir}"
    else
      echo "running: nothing"
    fi
    echo "queue: ${q:-empty}"
    [ -z "${waiting:-}" ] || echo "waiting: $waiting"
  } >"$next" && mv -f "$next" "$out/status.txt"
}

lock() {
  local pid
  if ! mkdir "$state/lock" 2>/dev/null; then
    pid=$(cat "$state/lock/pid" 2>/dev/null || true)
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
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
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    log "stopping job $pid left by the previous runner"
    kill_job "$pid"
  fi
  rm -f "$state/job.pid"
}

# shellcheck disable=SC2329 # the EXIT trap
on_exit() {
  if [ -n "${run_pid:-}" ]; then
    log "stopping; #$run_pr re-runs when the runner starts again"
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
    echo "local CI ($context): logging to $state/runner.log"
    exec >>"$state/runner.log" 2>&1
  fi
  trap on_exit EXIT
  trap 'exit 0' TERM INT HUP
  log "runner started: $context for $repo, checkout $checkout"
  kill_stale_job
  prs='' polled=false run_pr='' run_pid='' waiting='' next_poll=0
  local now
  while :; do
    [ -z "$run_pr" ] || check_job
    now=$(date +%s)
    if [ "$now" -ge "$next_poll" ]; then
      next_poll=$((now + poll_s))
      if poll; then
        sync_running
        # A replaced job changed statuses: read them again at once.
        [ "$next_poll" != 0 ] || continue
        queue_new
      fi
      prune
    fi
    [ -n "$run_pr" ] || start_next || true
    write_status
    if [ -n "$once" ] && $polled && [ -z "$run_pr" ] && [ "$next_poll" != 0 ] &&
      [ -z "$(queue)" ]; then
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
    <key>PATH</key><string>$PATH</string>
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

cmd_start() {
  require_macos start
  local a
  for a in "$web_agent" "$agent"; do
    [ -f "$(plist_path "$a")" ] || die "$a is not installed; run install"
    launchctl bootstrap "gui/$(id -u)" "$(plist_path "$a")" 2>/dev/null ||
      launchctl kickstart "gui/$(id -u)/$a"
  done
  echo "started $agent and $web_agent"
}

cmd_stop() {
  require_macos stop
  local a
  for a in "$agent" "$web_agent"; do
    launchctl bootout "gui/$(id -u)/$a" 2>/dev/null || true
  done
  echo "stopped $agent and $web_agent"
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
  {
    echo "# Written by scripts/local-ci.sh install; re-run it to change."
    printf 'LOCAL_CI_URL=%q\n' "https://$dns/ci"
    printf 'LOCAL_CI_PORT=%q\n' "$port"
  } >"$LOCAL_CI_HOME/runner.env"
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
    schtasks //query //tn "mdoc local CI" //fo list | grep -E '^(Status|Last Run Time):' ||
      echo "scheduled task 'mdoc local CI': not registered"
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
