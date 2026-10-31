#!/usr/bin/env bash
# Usage: scripts/test-local-ci.sh
#
# The local CI runner (scripts/local-ci.sh) against a local origin with
# refs/pull/N/merge refs, a fake gh that keeps commit statuses in a file and a
# fake just; no network, builds or real statuses.
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
# A real runner's job runs this too: start from no runner settings.
unset "${!LOCAL_CI_@}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
failures=0

check() { # description command...
  local what=$1
  shift
  if "$@"; then
    echo "ok: $what"
  else
    echo "FAIL: $what" >&2
    failures=$((failures + 1))
  fi
}

# --- paths-ignore, globs and the mobile step -------------------------------------------------

(
  export LOCAL_CI_SOURCED=1
  # shellcheck source=local-ci.sh
  source scripts/local-ci.sh
  globs=()
  while IFS= read -r g; do globs+=("$g"); done < <(paths_ignore <.github/workflows/ci.yml)
  check "ci.yml's paths-ignore list is read" [ "${#globs[@]}" -gt 0 ]
  check "it includes **/*.md" [ "${globs[0]}" = '**/*.md' ]
  # shellcheck disable=SC2317,SC2329 # called through eval
  ignored() { printf '%s\n' "$@" | only_ignored "${globs[@]}"; }
  check "root and nested Markdown are docs" ignored README.md docs/adr/0082-local-ci.md src/notes.md
  check "LICENSE and docs/ are docs" ignored LICENSE docs/evidence/run.json
  check "a source file is code" eval '! ignored README.md src/main.rs'
  check "ci.yml is code" eval '! ignored .github/workflows/ci.yml'
  check "docs.rs is not under docs/" eval '! ignored docs.rs'
  check "* does not cross directories" eval '! only_ignored "*.md" <<<"docs/a.md"'
  check "a dot is literal" eval '! only_ignored "build/appicon.svg" <<<"build/appiconxsvg"'
  check "no files is not docs only" eval '! only_ignored "**/*.md" </dev/null'
  # Step limits: the mobile step has its own, longer one.
  # shellcheck disable=SC2034 # read by timeout_for
  lint_s=1200 test_s=1800 mobile_s=2400
  check "lint keeps its limit" [ "$(timeout_for lint)" = 1200 ]
  check "setup shares lint's limit" [ "$(timeout_for setup)" = 1200 ]
  check "the mobile step has its own limit" [ "$(timeout_for mobile)" = 2400 ]
  check "test keeps its limit" [ "$(timeout_for test)" = 1800 ]
  # The mobile step follows the merge's main.
  repo=$(mktemp -d)
  g() { git -C "$repo" -c user.name=t -c user.email=t@example.com "$@"; }
  g init --quiet -b main
  echo a >"$repo/a" && g add a && g commit --quiet -m old-main
  old_main=$(g rev-parse HEAD)
  mkdir "$repo/scripts" && : >"$repo/scripts/mobile-check.sh"
  g add scripts && g commit --quiet -m new-main
  new_main=$(g rev-parse HEAD)
  # A PR merged onto the old main: no check to run.
  g checkout --quiet -b pr-old "$old_main" && echo b >"$repo/b" && g add b && g commit --quiet -m pr
  g checkout --quiet --detach "$old_main" && g merge --quiet --no-ff -m merge pr-old
  # shellcheck disable=SC2317,SC2329 # called through check
  in_repo() { (cd "$repo" && mobile_check_expected); }
  check "a merge ref on a main without the check skips it" eval '! in_repo'
  # A PR that deletes the script, merged onto the new main: the check still runs (and fails).
  g checkout --quiet -b pr-new "$new_main" && g rm --quiet scripts/mobile-check.sh && g commit --quiet -m drop
  g checkout --quiet --detach "$new_main" && g merge --quiet --no-ff -m merge pr-new
  check "a PR that removes the check still runs it" in_repo
  rm -rf "${repo:?}"
  # The Mac's start gate: memory pressure and disk, never swap in use.
  # shellcheck disable=SC2034 # read by capacity_ok and queued_text
  os=macos max_pressure=1 min_free_gb=40 waiting_kind=''
  fake_pressure=1 fake_free=100
  # shellcheck disable=SC2317,SC2329 # replace the real probes
  memory_pressure() { echo "$fake_pressure"; }
  # shellcheck disable=SC2317,SC2329
  disk_free_gb() { echo "$fake_free"; }
  capacity_ok >/dev/null
  check "normal pressure and room on disk start a job" [ -z "$waiting_kind" ]
  check "a free Mac's queued status is plain" [ "$(queued_text)" = Queued ]
  fake_pressure=2
  capacity_ok >/dev/null || true
  check "warning pressure holds jobs" [ "$waiting_kind" = memory ]
  check "the queued status says why" [ "$(queued_text)" = "Queued; waiting for memory pressure to drop (now warning)" ]
  fake_pressure=1 fake_free=39
  capacity_ok >/dev/null || true
  check "too little disk holds jobs" [ "$waiting_kind" = disk ]
  check "and says so, without a number that changes" [ "$(queued_text)" = "Queued; waiting for disk space (starts at 40 GB free)" ]
  # shellcheck disable=SC2016 # expands when eval runs it
  check "a new reason is logged" eval 'waiting_kind=""; [[ $(capacity_ok || true) == *"waiting: 39 GB free (starts at 40 GB)"* ]]'
  # shellcheck disable=SC2034
  max_pressure=2 fake_pressure=2 fake_free=100
  check "LOCAL_CI_MAX_PRESSURE=2 starts at warning" eval 'capacity_ok >/dev/null'
  check "other runners have no gate" eval 'os=windows fake_pressure=4 fake_free=0; capacity_ok >/dev/null'
  # Queued heads get the current text; running, fallback and final heads do not.
  posts=$(mktemp)
  # shellcheck disable=SC2317,SC2329
  post() { printf '%s\t%s\n' "$1" "$3" >>"$posts"; }
  # shellcheck disable=SC2034
  os=macos max_pressure=1 fake_pressure=2 run_pr=5 url=''
  capacity_ok >/dev/null || true
  line() { printf '%s\tfalse\tfalse\tUser\tMERGEABLE\tmain\th%s\t%s\t%s\t%s\n' "$1" "$1" "$2" "${3:-}" "${4:-}"; }
  prs=$(
    line 1 NONE
    line 2 PENDING 2026-01-01T00:00:00Z Queued
    line 3 PENDING 2026-01-01T00:00:00Z "Queued; waiting for memory pressure to drop (now warning)"
    line 4 PENDING 2026-01-01T00:00:00Z "Hosted fallback: hosted CI on h4 (attempt 1); x"
    line 5 PENDING 2026-01-01T00:00:00Z "Running just test on main@abc"
    line 6 SUCCESS 2026-01-01T00:00:00Z Passed
  )
  queue_new
  check "new and outdated queued heads get the waiting text" \
    [ "$(cut -f1 "$posts" | tr '\n' ' ')" = "h1 h2 " ]
  check "with the reason" grep -q "^h1	Queued; waiting for memory pressure" "$posts"
  check "the queue keeps the posted text" [ "$(awk -F'\t' '$1 == 2 { print $10 }' <<<"$prs")" = "$(queued_text)" ]
  # A runner that comes back tests heads the fallback holds or finished.
  prs=$(
    line 1 SUCCESS 2026-01-01T00:00:01Z "Hosted fallback: clippy and test (macos-26) passed; just mobile not run"
    line 2 ERROR 2026-01-01T00:00:02Z "Hosted fallback: no hosted result after 3 dispatches; dispatch ci.yml by hand"
    line 3 PENDING 2026-01-01T00:00:03Z "Hosted fallback: hosted CI on h3 (attempt 1); x"
    line 4 SUCCESS 2026-01-01T00:00:04Z "Passed on main@abc in 3m00s"
    line 6 FAILURE 2026-01-01T00:00:06Z "Failed in just test on main@abc"
  )
  check "fallback heads, finished or not, are queued; local results are not" \
    [ "$(queue | cut -f1 | tr '\n' ' ')" = "1 2 3 " ]
  rm -f "$posts"
  # install keeps the machine's own settings in runner.env.
  LOCAL_CI_HOME=$(mktemp -d) port=8787
  printf '%s\n' "# Written by scripts/local-ci.sh install; re-run it to change." \
    "LOCAL_CI_URL=https://old/ci" "LOCAL_CI_PORT=1" "LOCAL_CI_MIN_FREE_GB=30" >"$LOCAL_CI_HOME/runner.env"
  write_runner_env https://new/ci
  write_runner_env https://new/ci
  check "runner.env gets the new URL" grep -qx "LOCAL_CI_URL=https://new/ci" "$LOCAL_CI_HOME/runner.env"
  check "a machine setting survives re-installs" grep -qx "LOCAL_CI_MIN_FREE_GB=30" "$LOCAL_CI_HOME/runner.env"
  check "without duplicates" [ "$(wc -l <"$LOCAL_CI_HOME/runner.env" | tr -d ' ')" = 4 ]
  rm -rf "${LOCAL_CI_HOME:?}"
  exit "$failures"
) || failures=$((failures + $?))

# --- fakes --------------------------------------------------------------------

mkdir -p "$tmp/bin"
export FAKE="$tmp/fake"
mkdir -p "$FAKE"
: >"$FAKE/statuses.tsv"
: >"$FAKE/just.log"

# PRs: number, draft, cross-repository, author type, mergeable, base, head.
# Statuses: sha, state, description, target url, time.
cat >"$tmp/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -eu
if [ "$1 $2" = "api graphql" ]; then
  while IFS=$'\t' read -r n draft cross author mergeable base head; do
    latest=$(awk -F'\t' -v h="$head" '$1 == h { s = toupper($2); t = $5; d = $3 } END { print s "\t" t "\t" d }' "$FAKE/statuses.tsv")
    state=${latest%%$'\t'*}
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$n" "$draft" "$cross" "$author" \
      "$mergeable" "$base" "$head" "${state:-NONE}" "${latest#*$'\t'}"
  done <"$FAKE/prs.tsv"
  exit 0
fi
if [ "$1 $2 $3" = "api -X POST" ]; then
  sha=${4##*/}
  shift 4
  state= desc= url= context=
  while [ $# -gt 0 ]; do
    case "$2" in
      state=*) state=${2#state=} ;;
      description=*) desc=${2#description=} ;;
      target_url=*) url=${2#target_url=} ;;
      context=*) context=${2#context=} ;;
    esac
    shift 2
  done
  [ "$context" = "$FAKE_CONTEXT" ] || exit 7
  seq=$(($(wc -l <"$FAKE/statuses.tsv") + 1))
  printf '%s\t%s\t%s\t%s\t2026-01-01T00:%02d:%02dZ\n' "$sha" "$state" "$desc" "$url" $((seq / 60)) $((seq % 60)) >>"$FAKE/statuses.tsv"
  exit 0
fi
echo "fake gh: unexpected $*" >&2
exit 9
EOF

# Each PR's behavior is the ci-behavior file in its merge commit.
cat >"$tmp/bin/just" <<'EOF'
#!/usr/bin/env bash
set -eu
behavior=$(cat ci-behavior 2>/dev/null || echo none)
echo "$behavior $1 renders=${MDOC_HEADLESS_ARTIFACTS:+set}" >>"$FAKE/just.log"
[ -z "${CARGO_TARGET_DIR:-}" ] || exit 8
case "$behavior $1" in
  "fail test") exit 1 ;;
  "slow test") sleep 60 ;;
  "push lint")
    # A newer head arrives while this one runs.
    sed "s/$(cat "$FAKE/old-head")/$(cat "$FAKE/new-head")/" "$FAKE/prs.tsv" >"$FAKE/prs.new"
    mv "$FAKE/prs.new" "$FAKE/prs.tsv"
    sleep 60
    ;;
esac
[ "$1" != test ] || touch "$MDOC_HEADLESS_ARTIFACTS"
EOF
chmod +x "$tmp/bin/gh" "$tmp/bin/just"

# --- origin with PR merge refs --------------------------------------------------

git_() { git -C "$tmp/work" -c user.name=ci -c user.email=ci@example.com "$@"; }
git init --quiet --bare -b main "$tmp/origin.git"
git init --quiet -b main "$tmp/work"
mkdir -p "$tmp/work/.github/workflows" "$tmp/work/src" "$tmp/work/docs"
cp .github/workflows/ci.yml "$tmp/work/.github/workflows/ci.yml"
echo base >"$tmp/work/src/lib.rs"
echo readme >"$tmp/work/README.md"
echo doc >"$tmp/work/docs/a.md"
mkdir -p "$tmp/work/scripts"
: >"$tmp/work/scripts/mobile-check.sh"
cat >"$tmp/work/scripts/setup.sh" <<'EOF'
#!/usr/bin/env bash
# Setup must never wait on a prompt: the runner closes its stdin.
! read -r -t 1 _ || exit 5
echo "$(cat ci-behavior 2>/dev/null || echo none) setup" >>"$FAKE/just.log"
EOF
git_ add -A
git_ commit --quiet -m base
git_ push --quiet "$tmp/origin.git" main
base=$(git_ rev-parse HEAD)

# Commits a PR branch from main and its merge ref; prints the head.
pr() { # number behavior [file...]
  local n=$1 behavior=$2 f
  shift 2
  git_ checkout --quiet -B "pr-$n" "$base"
  echo "$behavior" >"$tmp/work/ci-behavior"
  [ "$behavior" != docs ] || rm "$tmp/work/ci-behavior"
  for f in "$@"; do echo "pr $n" >>"$tmp/work/$f"; done
  git_ add -A
  git_ commit --quiet -m "pr $n"
  local head
  head=$(git_ rev-parse HEAD)
  git_ checkout --quiet --detach "$base"
  git_ merge --quiet --no-ff -m "merge pr $n" "$head"
  git_ push --quiet --force "$tmp/origin.git" "HEAD:refs/pull/$n/merge"
  git_ rev-parse HEAD >"$tmp/merge-$head"
  echo "$head"
}
row() { printf '%s\tfalse\tfalse\tUser\tMERGEABLE\tmain\t%s\n' "$1" "$2"; }

pass=$(pr 1 pass src/lib.rs)
docs=$(pr 2 docs README.md docs/b.md)
conflict=$(pr 3 pass src/lib.rs)
draft=$(pr 4 pass src/lib.rs)
fork=$(pr 5 pass src/lib.rs)
bot=$(pr 6 pass src/lib.rs)
fail=$(pr 7 fail src/lib.rs)
slow=$(pr 8 slow src/lib.rs)
old=$(pr 9 push src/lib.rs)
done_before=$(pr 10 pass src/lib.rs)
stale=$(pr 11 pass src/lib.rs)
# PR 9 gets a newer head while its old one runs: its merge ref points at the
# old head until then.
new=$(pr 9 pass src/lib.rs src/new.rs)
new_merge=$(cat "$tmp/merge-$new")
git --git-dir="$tmp/origin.git" update-ref refs/pull/9/merge "$(cat "$tmp/merge-$old")"
echo "$old" >"$FAKE/old-head"
echo "$new" >"$FAKE/new-head"

{
  row 1 "$pass"
  row 2 "$docs"
  printf '3\tfalse\tfalse\tUser\tCONFLICTING\tmain\t%s\n' "$conflict"
  printf '4\ttrue\tfalse\tUser\tMERGEABLE\tmain\t%s\n' "$draft"
  printf '5\tfalse\ttrue\tUser\tMERGEABLE\tmain\t%s\n' "$fork"
  printf '6\tfalse\tfalse\tBot\tMERGEABLE\tmain\t%s\n' "$bot"
  row 7 "$fail"
  row 8 "$slow"
  row 9 "$old"
  row 10 "$done_before"
  row 11 "$stale"
} >"$FAKE/prs.tsv"
printf '%s\tsuccess\tPassed earlier\t\t2026-01-01T00:00:00Z\n' "$done_before" >>"$FAKE/statuses.tsv"
printf '%s\tpending\tRunning on main@0000000\t\t2026-01-01T00:00:00Z\n' "$stale" >>"$FAKE/statuses.tsv"

# --- run ------------------------------------------------------------------------

# shellcheck disable=SC2031 # only the paths-ignore subshell's config() sets it
export LOCAL_CI_HOME="$tmp/ci"
mkdir -p "$LOCAL_CI_HOME"
git clone --quiet "$tmp/origin.git" "$LOCAL_CI_HOME/mdoc"
case "$(platform)" in
  macos-arm64) FAKE_CONTEXT=local/macos ;;
  windows-x64) FAKE_CONTEXT=local/windows ;;
  *)
    export LOCAL_CI_OS=test
    FAKE_CONTEXT=local/test
    ;;
esac
export FAKE_CONTEXT
export LOCAL_CI_REPO=example/mdoc LOCAL_CI_URL=https://ci.example/ci
# A 15 s test limit: room for a passing fake step while just test runs this
# beside nextest (scripts/test.sh); the slow one sleeps 60 s.
export LOCAL_CI_POLL=1 LOCAL_CI_TICK=1 LOCAL_CI_TEST_TIMEOUT=15
export LOCAL_CI_MIN_FREE_GB=0 LOCAL_CI_MAX_PRESSURE=4
export CARGO_TARGET_DIR="$tmp/shared-target"
# As in hosted CI: only the runner's test step may name the renders folder.
export MDOC_HEADLESS_ARTIFACTS="$tmp/outer-renders"
export PATH="$tmp/bin:$PATH"

# Once PR 9's old head is running, move its merge ref to the new head.
(
  for _ in $(seq 1 300); do
    if grep -q "^push lint" "$FAKE/just.log" 2>/dev/null; then
      git --git-dir="$tmp/origin.git" update-ref refs/pull/9/merge "$new_merge"
      exit 0
    fi
    sleep 0.2
  done
) &
mover=$!

# An older head of PR 1 tested earlier, with its renders.
mkdir -p "$LOCAL_CI_HOME/out/pr-1/0000000/renders"
echo log >"$LOCAL_CI_HOME/out/pr-1/0000000/log.txt"

# A previous runner's lock and job pid, now an unrelated process: the
# runner takes the lock over and leaves that process alone.
mkdir -p "$LOCAL_CI_HOME/state/lock"
# Outlives the runner's 3-minute allowance; killed below.
sleep 600 &
unrelated=$!
echo "$unrelated" >"$LOCAL_CI_HOME/state/lock/pid"
echo "$unrelated" >"$LOCAL_CI_HOME/state/job.pid"

# The build cache survives a job's clean; other untracked files do not.
mkdir -p "$LOCAL_CI_HOME/mdoc/target/debug"
echo cache >"$LOCAL_CI_HOME/mdoc/target/debug/kept"
echo stray >"$LOCAL_CI_HOME/mdoc/stray.txt"

bash scripts/local-ci.sh once >"$tmp/runner.log" 2>&1 &
runner=$!
for _ in $(seq 1 180); do
  kill -0 "$runner" 2>/dev/null || break
  sleep 1
done
if kill -0 "$runner" 2>/dev/null; then
  kill "$runner"
  echo "FAIL: the runner did not finish within 3 minutes" >&2
  failures=$((failures + 1))
fi
rc=0
wait "$runner" || rc=$?
kill "$mover" 2>/dev/null || true
wait "$mover" 2>/dev/null || true
check "the runner exits once nothing is queued" [ "$rc" -eq 0 ]
check "a reused pid in the lock or job.pid is left alone" kill -0 "$unrelated"
kill "$unrelated" 2>/dev/null || true
wait "$unrelated" 2>/dev/null || true

# The last status posted on a head: "state<TAB>description<TAB>url".
last() { awk -F'\t' -v h="$1" -v OFS='\t' '$1 == h { s = $2; d = $3; u = $4 } END { print s, d, u }' "$FAKE/statuses.tsv"; }
# shellcheck disable=SC2053 # $2 is a glob
has() { [[ $(last "$1") == $2 ]]; }
short_base=${base:0:7}

check "a passing PR gets success naming the base" has "$pass" "success	Passed on main@$short_base in *	https://ci.example/ci/pr-1/${pass:0:7}/"
check "it was queued, then ran" grep -q "^$pass	pending	Running on main@$short_base	https://ci.example/ci/pr-1/" "$FAKE/statuses.tsv"
check "a step the runner sees running posts its own status" grep -q "^$slow	pending	Running just test on main@$short_base	https://ci.example/ci/pr-8/" "$FAKE/statuses.tsv"
check "queued statuses link the runner's status" grep -q "^$pass	pending	Queued	https://ci.example/ci/status.txt" "$FAKE/statuses.tsv"
steps_run() { grep "^$1 " "$FAKE/just.log" | head -3 | cut -d' ' -f2 | tr '\n' ' '; }
check "setup, lint, then test" [ "$(steps_run pass)" = "setup lint test " ]
case "$FAKE_CONTEXT" in
  local/macos) last_step="mobile" ;;
  *) last_step="test" ;;
esac
check "the Mac ends with the mobile check, other runners with test" \
  [ "$(grep "^pass " "$FAKE/just.log" | tail -1 | cut -d' ' -f2)" = "$last_step" ]
check "lint gets no renders" grep -qx "pass lint renders=" "$FAKE/just.log"
check "the test step gets MDOC_HEADLESS_ARTIFACTS" grep -qx "pass test renders=set" "$FAKE/just.log"
check "the job's log and summary are kept" [ -s "$LOCAL_CI_HOME/out/pr-1/${pass:0:7}/log.txt" ]
check "the summary records the result" grep -q "^result: success" "$LOCAL_CI_HOME/out/pr-1/${pass:0:7}/summary.txt"
check "a docs-only PR passes without a build" has "$docs" "success	Docs only, not tested (main@$short_base)	"
check "docs-only never ran just" eval "! grep -q '^none' \"\$FAKE/just.log\""
check "a conflicting PR fails" has "$conflict" "failure	Conflicts with main	"
for skipped in "$draft" "$fork" "$bot"; do
  check "drafts, forks and bots get no status (${skipped:0:7})" eval "! grep -q '^$skipped' \"\$FAKE/statuses.tsv\""
done
check "a failing test fails, naming the step" has "$fail" "failure	Failed in just test on main@$short_base	*"
check "a test over its limit times out" has "$slow" "failure	Timed out in just test after 0m15s on main@$short_base	*"
check "a newer head supersedes a running job" has "$old" "error	Superseded by ${new:0:7}	*"
check "the newer head is tested" has "$new" "success	Passed on main@$short_base in *"
check "a tested head is not re-run" has "$done_before" "success	Passed earlier	"
check "a pending head left by a stopped runner re-runs" has "$stale" "success	Passed on main@$short_base in *"
check "renders are kept for a PR's newest head only" [ ! -e "$LOCAL_CI_HOME/out/pr-1/0000000/renders" ]
check "an older head's log is kept" [ -e "$LOCAL_CI_HOME/out/pr-1/0000000/log.txt" ]
check "the status file is written" grep -q "^running: nothing" "$LOCAL_CI_HOME/out/status.txt"
check "the lock is released" [ ! -e "$LOCAL_CI_HOME/state/lock" ]
check "a job keeps target/" [ -e "$LOCAL_CI_HOME/mdoc/target/debug/kept" ]
check "a job removes other untracked files" [ ! -e "$LOCAL_CI_HOME/mdoc/stray.txt" ]

if [ "$failures" -gt 0 ]; then
  echo "--- runner log" >&2
  cat "$tmp/runner.log" >&2
  echo "--- statuses" >&2
  cat "$FAKE/statuses.tsv" >&2
  die "$failures local CI check(s) failed"
fi
echo "local CI runner: all checks passed"
