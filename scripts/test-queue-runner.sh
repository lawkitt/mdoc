#!/usr/bin/env bash
# Usage: scripts/test-queue-runner.sh
#
# The merge-queue runner (scripts/queue-runner.sh) against a local origin
# with queue branches, a fake gh that lists them from that origin and keeps
# commit statuses in a file, and a fake just; no network, builds or real
# statuses. Bash 3.2 compatible (macOS /bin/bash runs it).
# Checks given to eval expand when they run; the subshells test functions
# with their own settings.
# shellcheck disable=SC2016,SC2030,SC2031
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
# A real runner's job runs this too: start from no runner settings.
unset "${!QUEUE_CI_@}" "${!LOCAL_CI_@}"
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

not() { ! "$@"; }

# A time $1 days ago, as touch -t takes it.
days_ago() { date -v-"$1"d +%Y%m%d%H%M 2>/dev/null || date -d "$1 days ago" +%Y%m%d%H%M; }

# --- functions ------------------------------------------------------------------

(
  # Its own count, which the parent adds.
  failures=0
  export QUEUE_RUNNER_SOURCED=1
  # shellcheck source=queue-runner.sh
  source scripts/queue-runner.sh
  check "the steps read as their commands" [ "$(label setup), $(label lint), $(label test), $(label android)" = "scripts/setup.sh, just lint, just test, just android" ]
  check "durations read as minutes and seconds" [ "$(duration 1805)" = 30m05s ]
  for j in windows android; do
    (
      # Not /e/ci: on the PC that is the installed runners' home, whose
      # runner.env config() reads.
      export QUEUE_CI_JOB=$j LOCAL_CI_HOME=/nonexistent/ci LOCAL_CI_URL=https://pc.example/ci
      config
      case "$j" in
        windows) want="ci/windows desktop setup lint test 1800" ;;
        *) want="ci/android android setup android 2400" ;;
      esac
      check "the $j job's context, area, steps and limit" [ "$context $area $steps $limit_s" = "$want" ]
      check "its own home, output and link" \
        [ "$home $out $url" = "/nonexistent/ci/queue-$j /nonexistent/ci/out/queue-$j https://pc.example/ci/queue-$j" ]
      check "a 4-minute heartbeat, 40 GB floor, 60 GB target/, 20 GB of output" \
        [ "$heartbeat_s $min_free_gb $target_max_gb $out_max_kb" = "240 40 60 $((20 * 1048576))" ]
    )
  done
  check "no job is refused" eval '! (QUEUE_CI_JOB=macos config) 2>/dev/null'

  # nextest's failing tests, once each, without colours or retry counts.
  esc=$(printf '\033')
  {
    echo "        PASS [   0.010s] mdoc-core tests::fine"
    echo "        FAIL [   0.010s] mdoc-core tests::broken"
    echo "${esc}[31;1m        FAIL${esc}[0m [   0.020s] (1/2) mdoc workspace::tests::flaky"
    echo "     TIMEOUT [ 120.000s] mdoc workspace::tests::slow"
    echo "        FAIL [   0.010s] mdoc-core tests::broken"
  } >"$tmp/nextest.log"
  check "the failing tests are named once each" \
    [ "$(failed_tests "$tmp/nextest.log" | LC_ALL=C sort | tr '\n' ,)" = "mdoc workspace::tests::flaky,mdoc workspace::tests::slow,mdoc-core tests::broken," ]

  # Gradle's JUnit XML: <testcase> as `/>`, ` />` and `>`, a failure and an
  # error inside the last kind, passing JVM tests elsewhere.
  mkdir -p "$tmp/gradle/connected" "$tmp/gradle/unit"
  cat >"$tmp/gradle/connected/TEST-mdoc-ci(AVD) - 16-_app-.xml" <<'XML'
<?xml version='1.0' encoding='UTF-8' ?>
<testsuite name="io.github.lawkitt.mdoc.IdentitiesTest" tests="4" failures="1" errors="1">
  <testcase name="aTapOpensAnIdentity" classname="io.github.lawkitt.mdoc.IdentitiesTest" time="1.2"/>
  <testcase name="aLongPressKeepsAllOfAnIdentity" classname="io.github.lawkitt.mdoc.IdentitiesTest" time="10.1">
    <failure>androidx.compose.ui.test.ComposeTimeoutException: Condition still not satisfied after 10000 ms</failure>
  </testcase>
  <testcase name="aSwipeKeeps" classname="io.github.lawkitt.mdoc.IdentitiesTest" time="0.3" />
  <testcase name="pasteFillsTheText" classname="io.github.lawkitt.mdoc.PasteSheetTest" time="0.4">
    <error message="null">java.lang.NullPointerException</error>
  </testcase>
</testsuite>
XML
  printf '%s\n' '<testsuite tests="1" failures="0">' '  <testcase name="parity" classname="io.github.lawkitt.mdoc.CoreParityTest" time="0.1"/>' '</testsuite>' \
    >"$tmp/gradle/unit/TEST-io.github.lawkitt.mdoc.CoreParityTest.xml"
  check "Gradle's failing tests are named from its XML" \
    [ "$(gradle_failed_tests "$tmp/gradle/connected" "$tmp/gradle/unit" "$tmp/gradle/none" | tr '\n' ,)" = \
    "android:io.github.lawkitt.mdoc.IdentitiesTest#aLongPressKeepsAllOfAnIdentity,android:io.github.lawkitt.mdoc.PasteSheetTest#pasteFillsTheText," ]
  check "and none from folders Gradle did not write" [ -z "$(gradle_failed_tests "$tmp/gradle/none")" ]

  # A failed job's summary ends with its log's last 60 lines, plain.
  mkdir -p "$tmp/excerpt"
  {
    seq 1 100
    echo "${esc}[31mFAILURE${esc}[0m: Build failed with an exception.$(printf '\r')"
  } >"$tmp/excerpt/log.txt"
  log_excerpt "$tmp/excerpt" >"$tmp/excerpt.txt"
  check "the log excerpt is its last 60 lines" \
    eval '[ "$(wc -l <"$tmp/excerpt.txt" | tr -d " ")" = 61 ] && [ "$(sed -n 2p "$tmp/excerpt.txt")" = 42 ]'
  check "without colours or carriage returns" [ "$(tail -n 1 "$tmp/excerpt.txt")" = "FAILURE: Build failed with an exception." ]

  # Output: older than 7 days goes, then the oldest while over the size.
  # shellcheck disable=SC2034 # read by prune_out
  out=$tmp/prune keep_days=7 out_max_kb=700
  for d in old a b c; do
    mkdir -p "$out/$d"
    head -c 307200 /dev/zero >"$out/$d/log.txt"
  done
  echo heartbeat >"$out/status.txt"
  touch -t "$(days_ago 8)" "$out/old"
  touch -t "$(days_ago 3)" "$out/a"
  touch -t "$(days_ago 2)" "$out/b"
  touch -t "$(days_ago 1)" "$out/c"
  prune_out >"$tmp/prune.log"
  check "output older than 7 days is deleted" [ ! -e "$out/old" ]
  check "then the oldest folder while the output is over its size" [ ! -e "$out/a" ]
  check "and no more" eval '[ -d "$out/b" ] && [ -d "$out/c" ] && [ -f "$out/status.txt" ]'
  check "each deletion is logged" [ "$(grep -c '^.* deleting ' "$tmp/prune.log")" = 2 ]

  # status.txt while the queue cannot be read, then once it can.
  # shellcheck disable=SC2034 # read by write_status
  out=$tmp/status context=ci/windows run_pid='' queued='' waiting='' update_failed='' unsettled=''
  # Not checked yet: the tailnet checks are below.
  # shellcheck disable=SC2034
  tailnet="" tailnet_at=$(date +%s) tailnet_s=600
  mkdir -p "$out"
  # shellcheck disable=SC2034
  offline_since=1 offline_at=2026-11-04T02:29:00Z offline_why="could not list the queue on GitHub"
  write_status
  check "an unreadable queue is not shown as empty" grep -q "^queue: unknown (nothing known)" "$out/status.txt"
  check "status.txt says since when GitHub cannot be read" \
    grep -q "^offline: could not list the queue on GitHub since 2026-11-04T02:29:00Z; no status can be posted" "$out/status.txt"
  offline_since=''
  write_status
  check "and stops once it can" eval 'grep -q "^queue: empty" "$out/status.txt" && ! grep -q "^offline:" "$out/status.txt"'

  # The tailnet page: a fake tailscale whose serve config has /ci (beside
  # another app's path), or not.
  # shellcheck disable=SC2034 # read by check_tailnet
  tailnet='' tailnet_at=0 tailnet_s=600 net_s=5 tailscale_cmd=$tmp/fake-tailscale
  cat >"$tailscale_cmd" <<EOF
#!/usr/bin/env bash
echo "\$*" >>"$tmp/tailscale.log"
[ ! -e "$tmp/serve-off" ] || { echo '{}'; exit 0; }
echo '{"Web":{"pc.example.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:3000"},"/ci":{"Proxy":"http://127.0.0.1:8787"}}}}}'
EOF
  chmod +x "$tailscale_cmd"
  touch "$tmp/serve-off"
  write_status
  check "status.txt says when the tailnet does not serve /ci" grep -qx "tailnet: /ci not served" "$out/status.txt"
  rm -f "$tmp/serve-off"
  write_status
  check "and checks again at most every 10 minutes" \
    eval 'grep -qx "tailnet: /ci not served" "$out/status.txt" && [ "$(wc -l <"$tmp/tailscale.log" | tr -d " ")" = 1 ]'
  tailnet_at=$(($(date +%s) - 600))
  write_status
  check "then sees /ci served again" eval '! grep -q "^tailnet:" "$out/status.txt" && [ "$(sort -u "$tmp/tailscale.log")" = "serve status --json" ]'
  tailscale_cmd=$tmp/no-tailscale tailnet_at=0
  write_status
  check "with no tailscale (macOS and Linux) it says nothing" not grep -q "^tailnet:" "$out/status.txt"
  exit "$failures"
) || failures=$((failures + $?))

# --- fakes ------------------------------------------------------------------------

mkdir -p "$tmp/bin"
export FAKE="$tmp/fake" FAKE_ORIGIN="$tmp/origin.git"
mkdir -p "$FAKE"
: >"$FAKE/statuses.tsv"
: >"$FAKE/just.log"

# The queue branches come from the origin repository; PRs from prs.tsv
# (number, author type, cross-repository); statuses go to statuses.tsv (sha,
# context, state, description, target url).
cat >"$tmp/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -eu
[ "$1" = api ] || { echo "fake gh: unexpected $*" >&2; exit 9; }
shift
[ "$1" != --paginate ] || shift
if [ "$1" = -X ]; then
  sha=${3##*/}
  shift 3
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
  printf '%s\t%s\t%s\t%s\t%s\n' "$sha" "$context" "$state" "$desc" "$url" >>"$FAKE/statuses.tsv"
  exit 0
fi
case "$1" in
  */git/matching-refs/heads/*)
    # GitHub stops answering once, mid-job.
    if [ -e "$FAKE/stall" ]; then
      rm -f "$FAKE/stall"
      exec sleep 30
    fi
    p=${1#*/git/matching-refs/heads/}
    git --git-dir="$FAKE_ORIGIN" for-each-ref --format='%(refname:lstrip=2) %(objectname)' "refs/heads/${p%/}"
    ;;
  */pulls/*)
    # GitHub cannot be reached for this PR the first $(cat pr-down-N) reads.
    down=$FAKE/pr-down-${1##*/}
    if [ -s "$down" ] && [ "$(cat "$down")" -gt 0 ]; then
      echo $(($(cat "$down") - 1)) >"$down"
      echo "error connecting to api.github.com" >&2
      exit 1
    fi
    awk -v n="${1##*/}" '$1 == n { print $2, $3; found = 1 } END { exit !found }' "$FAKE/prs.tsv" ||
      { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
    ;;
  *) echo "fake gh: unexpected $1" >&2; exit 9 ;;
esac
EOF

# Each queue commit's behavior is its ci-behavior file: under apps/android/
# for an Android change, else at the root (a desktop file).
cat >"$tmp/bin/just" <<'EOF'
#!/usr/bin/env bash
set -eu
behavior=$({ cat apps/android/ci-behavior ci-behavior 2>/dev/null || true; } | head -n 1)
target=gone
[ ! -e target/kept ] || target=kept
echo "${behavior:-none} $* renders=${MDOC_HEADLESS_ARTIFACTS:-} job=${MDOC_ANDROID_JOB:-} target=$target" >>"$FAKE/just.log"
[ -z "${CARGO_TARGET_DIR:-}" ] || exit 8
drop_branch() {
  local r
  for r in $(git --git-dir="$FAKE_ORIGIN" for-each-ref --points-at "$(git rev-parse HEAD)" --format='%(refname)' refs/heads/gh-readonly-queue refs/heads/ci-shadow); do
    git --git-dir="$FAKE_ORIGIN" update-ref -d "$r"
  done
}
# Before a step fails: a failed job keeps its renders.
case "$1" in
  test) mkdir -p "$MDOC_HEADLESS_ARTIFACTS" && echo '{}' >"$MDOC_HEADLESS_ARTIFACTS/summary.json" ;;
  android) mkdir -p apps/android/app/build/reports && echo report >apps/android/app/build/reports/index.html ;;
esac
case "$behavior $1" in
  "fail test")
    echo "        FAIL [   0.010s] mdoc-core tests::broken"
    echo "        FAIL [   0.020s] mdoc workspace::tests::flaky"
    exit 100
    ;;
  # Runs until the runner has refreshed its pending status once (or 50 s,
  # under the 60 s limit): a fixed sleep raced the runner's polls, which
  # take seconds each on Windows beside nextest (#532, #585).
  "beat test")
    sha=$(git rev-parse HEAD)
    for _ in $(seq 1 50); do
      [ "$(awk -F'\t' -v h="$sha" '$1 == h && index($4, "Running just test (") == 1' "$FAKE/statuses.tsv" | wc -l)" -lt 2 ] || break
      sleep 1
    done
    ;;
  "slow test")
    touch "$FAKE/stall"
    sleep 120
    ;;
  "vanish lint")
    drop_branch
    sleep 120
    ;;
  "merged lint")
    git --git-dir="$FAKE_ORIGIN" update-ref refs/heads/main "$(git rev-parse HEAD)"
    drop_branch
    sleep 120
    ;;
  # A new runner reaches main while this job runs.
  "bump lint") git --git-dir="$FAKE_ORIGIN" update-ref refs/heads/main "$(cat "$FAKE/next-main")" ;;
  # An emulator test fails: Gradle's log names no test, its XML does.
  "gradlefail android")
    r=apps/android/app/build/outputs/androidTest-results/connected/debug
    u=apps/android/app/build/test-results/testDebugUnitTest
    mkdir -p "$r" "$u"
    printf '%s\n' '<testsuite>' '  <testcase name="aLongPressKeepsAllOfAnIdentity" classname="io.github.lawkitt.mdoc.IdentitiesTest" time="10">' \
      '    <failure>ComposeTimeoutException</failure>' '  </testcase>' '</testsuite>' >"$r/TEST-mdoc-ci.xml"
    printf '%s\n' '<testsuite>' '  <testcase name="parity" classname="io.github.lawkitt.mdoc.CoreParityTest" time="0.1"/>' '</testsuite>' >"$u/TEST-parity.xml"
    echo "> Task :app:connectedDebugAndroidTest FAILED"
    printf '\033[31mFAILURE\033[0m: Build failed with an exception.\n'
    echo "> There were failing tests. See the report at: file:///ci/index.html"
    exit 1
    ;;
esac
EOF
# tailscale serve's config: /ci served unless $FAKE/serve-off exists.
cat >"$tmp/bin/tailscale" <<'EOF'
#!/usr/bin/env bash
[ "$*" = "serve status --json" ] || { echo "fake tailscale: unexpected $*" >&2; exit 9; }
[ ! -e "$FAKE/serve-off" ] || { echo '{}'; exit 0; }
echo '{"Web":{"pc.example.ts.net:443":{"Handlers":{"/ci":{"Proxy":"http://127.0.0.1:8787"}}}}}'
EOF
# The results page from the tailnet: the run's own output folder, unless
# $FAKE/page-down exists (the web server stopped).
cat >"$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
[ ! -e "$FAKE/page-down" ] || { echo "curl: (7) Failed to connect" >&2; exit 7; }
for u; do :; done
case "$u" in
  https://ci.example/ci/*) cat "$LOCAL_CI_HOME/out/${u#https://ci.example/ci/}" ;;
  *) exit 6 ;;
esac
EOF
chmod +x "$tmp/bin/gh" "$tmp/bin/just" "$tmp/bin/tailscale" "$tmp/bin/curl"

# --- origin with queue branches -------------------------------------------------

git_() { git -C "$tmp/work" -c user.name=ci -c user.email=ci@example.com "$@"; }
git init --quiet --bare -b main "$FAKE_ORIGIN"
git init --quiet -b main "$tmp/work"
mkdir -p "$tmp/work/src" "$tmp/work/docs" "$tmp/work/scripts" "$tmp/work/apps/android"
tr -d '\r' <scripts/ci-areas.txt >"$tmp/work/scripts/ci-areas.txt"
# main's runner scripts, which an installed copy updates itself from.
cp scripts/queue-runner.sh scripts/lib.sh "$tmp/work/scripts/"
echo '[toolchain]' >"$tmp/work/rust-toolchain.toml"
echo base >"$tmp/work/src/lib.rs"
echo base >"$tmp/work/apps/android/App.kt"
echo readme >"$tmp/work/README.md"
echo doc >"$tmp/work/docs/a.md"
cat >"$tmp/work/scripts/setup.sh" <<'EOF'
#!/usr/bin/env bash
# Setup must never wait on a prompt: the runner closes its stdin.
! read -r -t 1 _ || exit 5
behavior=$({ cat apps/android/ci-behavior ci-behavior 2>/dev/null || true; } | head -n 1)
echo "${behavior:-none} setup" >>"$FAKE/just.log"
# A download that fails once.
if [ "$behavior" = setupflake ] && [ ! -e "$FAKE/setup-flaked" ]; then
  touch "$FAKE/setup-flaked"
  exit 1
fi
EOF
git_ add -A
GIT_COMMITTER_DATE="1767225600 +0000" git_ commit --quiet -m base
git_ push --quiet "$FAKE_ORIGIN" main
base=$(git_ rev-parse HEAD)

# A queue commit on main, made $1 * 10 s after main, on a branch under
# $prefix (default gh-readonly-queue/main/); its subject names PR #$1. The
# behavior goes in $at (default the root, a desktop file). Prints the commit.
qc() { # pr behavior [file...]
  local n=$1 behavior=$2 f head
  shift 2
  git_ checkout --quiet -B queue "$base"
  [ "$behavior" = docs ] || echo "$behavior" >"$tmp/work/${at:-ci-behavior}"
  for f in "$@"; do echo "pr $n" >>"$tmp/work/$f"; done
  git_ add -A
  GIT_COMMITTER_DATE="$((1767225600 + n * 10)) +0000" git_ commit --quiet -m "$behavior (#$n)"
  head=$(git_ rev-parse HEAD)
  git_ push --quiet "$FAKE_ORIGIN" "HEAD:refs/heads/${prefix:-gh-readonly-queue/main/}pr-$n-$head"
  echo "$head"
}

# Made out of order: the runner takes them by commit time.
fail=$(qc 7 fail src/lib.rs)
pass=$(qc 1 pass src/lib.rs)
docs=$(qc 2 docs README.md docs/b.md)
kotlin=$(at=apps/android/ci-behavior qc 3 kotlin apps/android/App.kt)
fork=$(qc 4 fork src/lib.rs)
bot=$(qc 5 bot src/lib.rs)
unknown=$(qc 6 unknown src/lib.rs)
beat=$(prefix=ci-shadow/main/ qc 8 beat src/lib.rs)
slow=$(qc 9 slow src/lib.rs)
vanish=$(qc 10 vanish src/lib.rs)
toolchain=$(qc 11 toolchain rust-toolchain.toml src/lib.rs)
merged=$(qc 12 merged src/lib.rs)
flake=$(qc 14 setupflake src/lib.rs)
# Its PR cannot be read at first: GitHub is unreachable for two reads.
unread=$(qc 15 unread src/lib.rs)
echo 2 >"$FAKE/pr-down-15"
gradlefail=$(at=apps/android/ci-behavior qc 19 gradlefail apps/android/App.kt)
# The CI machinery alone (ADR 0100): ci/scripts runs it, hosted.
gate=$(at=scripts/merge-gate.sh qc 20 gate .github/workflows/ci.yml)
{
  for n in 1 2 3 7 8 9 10 11 12 14 15 19 20; do echo "$n User false"; done
  echo "4 User true"
  echo "5 Bot false"
} >"$FAKE/prs.tsv"

# --- the Windows job --------------------------------------------------------------

export LOCAL_CI_HOME="$tmp/ci" LOCAL_CI_URL=https://ci.example/ci QUEUE_CI_REPO=example/mdoc
# A 60 s job limit, for the whole job: on the PC beside nextest (scripts/test.sh)
# the heartbeat job's checkout, setup and lint took 16 s and its refreshed
# status came 12 s after the first (c066ec0, #585), so 30 s left too little.
# The slow, dropped and merged ones sleep 120 s, past it.
export QUEUE_CI_POLL=1 QUEUE_CI_TICK=1 QUEUE_CI_HEARTBEAT=1 QUEUE_CI_TIMEOUT=60 QUEUE_CI_MIN_FREE_GB=0
# A stalled gh call ends after 3 s (the fake stalls for 30).
export QUEUE_CI_NET_TIMEOUT=3
export CARGO_TARGET_DIR="$tmp/shared-target"
# As in hosted CI: only the runner's test step may name the renders folder.
export MDOC_HEADLESS_ARTIFACTS="$tmp/outer-renders"
export PATH="$tmp/bin:$PATH"

W=ci/windows A=ci/android
wout=$LOCAL_CI_HOME/out/queue-windows
git clone --quiet "$FAKE_ORIGIN" "$LOCAL_CI_HOME/queue-windows/mdoc"
# The build cache, kept unless the toolchain changes; a stray file is not.
mkdir -p "$LOCAL_CI_HOME/queue-windows/mdoc/target/debug"
echo cache >"$LOCAL_CI_HOME/queue-windows/mdoc/target/kept"
echo stray >"$LOCAL_CI_HOME/queue-windows/mdoc/stray.txt"
# Output from earlier runs: one a week old, one recent.
mkdir -p "$wout/0ld0000" "$wout/recent0"
touch -t "$(days_ago 8)" "$wout/0ld0000"

run_once() { # job log [script]
  QUEUE_CI_JOB=$1 bash "${3:-scripts/queue-runner.sh}" once >"$2" 2>&1 &
  local runner=$! rc=0
  for _ in $(seq 1 300); do
    kill -0 "$runner" 2>/dev/null || break
    sleep 1
  done
  if kill -0 "$runner" 2>/dev/null; then
    kill "$runner"
    echo "FAIL: the $1 runner did not finish within 5 minutes" >&2
    failures=$((failures + 1))
  fi
  wait "$runner" || rc=$?
  check "the $1 runner exits once nothing is queued" [ "$rc" -eq 0 ]
}
run_once windows "$tmp/windows.log"

# The last status posted on a commit for a context: "state<TAB>description<TAB>url".
last() { awk -F'\t' -v h="$1" -v c="$2" -v OFS='\t' '$1 == h && $2 == c { s = $3; d = $4; u = $5 } END { print s, d, u }' "$FAKE/statuses.tsv"; }
# shellcheck disable=SC2053 # $3 is a glob
has() { [[ $(last "$1" "$2") == $3 ]]; }
posts() { awk -F'\t' -v h="$1" -v c="$2" '$1 == h && $2 == c' "$FAKE/statuses.tsv"; }
ran() { grep "^$1 " "$FAKE/just.log" | cut -d' ' -f2 | tr '\n' ' '; }
first() { grep -n "^$1 " "$FAKE/just.log" | head -1 | cut -d: -f1; }

check "a passing commit gets success with its time and log folder" \
  has "$pass" "$W" "success	Passed in *	https://ci.example/ci/queue-windows/${pass:0:7}/"
check "it was pending from the start" grep -q "^$pass	$W	pending	Running checkout (0m00s)	https://ci.example/ci/queue-windows/${pass:0:7}/" "$FAKE/statuses.tsv"
check "setup, lint, then test" [ "$(ran pass)" = "setup lint test " ]
check "lint gets no renders" grep -q "^pass lint renders= job=" "$FAKE/just.log"
check "test writes its renders in the run folder" grep -q "^pass test renders=$wout/${pass:0:7}/renders job=" "$FAKE/just.log"
check "which a passing job deletes" [ ! -e "$wout/${pass:0:7}/renders" ]
check "saying so in the summary" grep -q "^upkeep: deleted renders/ (the job passed)" "$wout/${pass:0:7}/summary.txt"
check "the log and summary are kept" eval '[ -s "$wout/${pass:0:7}/log.txt" ] && grep -q "^result: success" "$wout/${pass:0:7}/summary.txt"'
check "a passing job's summary has no log excerpt" not grep -q "^--- log.txt" "$wout/${pass:0:7}/summary.txt"
check "a docs-only commit passes without a build" has "$docs" "$W" "success	Docs only	"
check "and is never pending" not eval 'posts "$docs" "$W" | grep -q pending'
check "an Android-only commit leaves ci/windows not affected" has "$kotlin" "$W" "success	Not affected: no core or desktop change	"
check "without a build" [ -z "$(ran kotlin)" ]
check "a CI-only commit leaves ci/windows not affected" has "$gate" "$W" "success	Not affected: no core or desktop change	"
check "without a build either" [ -z "$(ran gate)" ]
for c in "$fork" "$bot" "$unknown"; do
  check "a group with a fork, bot or unknown PR gets no status (${c:0:7})" not grep -q "^$c" "$FAKE/statuses.tsv"
done
check "nor a build" [ -z "$(ran fork)$(ran bot)$(ran unknown)" ]
check "and is logged" [ "$(grep -c 'third-party code' "$tmp/windows.log")" = 3 ]
check "a PR GitHub cannot be read for is retried at the next poll" \
  grep -q "${unread:0:7} .*could not read #15: error connecting to api.github.com; retrying at the next poll" "$tmp/windows.log"
check "not taken for third-party code" not grep -q "${unread:0:7} .*third-party code" "$tmp/windows.log"
check "and is built once it can be read" has "$unread" "$W" "success	Passed in *"
check "a failing test fails, naming the step and the tests" \
  has "$fail" "$W" "failure	Failed in just test: mdoc* and 1 more	https://ci.example/ci/queue-windows/${fail:0:7}/"
check "which are listed in the run folder" [ "$(wc -l <"$wout/${fail:0:7}/failed-tests.txt" | tr -d ' ')" = 2 ]
check "a failed job keeps its renders" [ -f "$wout/${fail:0:7}/renders/summary.json" ]
check "its summary ends with the log's last lines, after the result" \
  eval 'sed -n "/^result: failure/,\$p" "$wout/${fail:0:7}/summary.txt" | sed -n 2p | grep -q "^--- log.txt, last 60 lines" &&
  grep -q "FAIL \[   0.010s\] mdoc-core tests::broken" "$wout/${fail:0:7}/summary.txt"'
check "as does a timed-out job's" grep -q "^--- log.txt, last 60 lines" "$wout/${slow:0:7}/summary.txt"
check "status.txt has no tailnet line while /ci is served" not grep -q "^tailnet:" "$wout/status.txt"
check "a job refreshes its pending status while it runs (the heartbeat)" \
  [ "$(posts "$beat" "$W" | grep -c "pending	Running just test (")" -ge 2 ]
check "a ci-shadow/main/ commit is tested too" has "$beat" "$W" "success	Passed in *"
check "a job over its limit is killed and times out" has "$slow" "$W" "failure	Timed out in just test after 1m00s	*"
check "a stalled gh call during it is ended, and the next poll goes on" \
  grep -q "warning: could not list the queue" "$tmp/windows.log"
check "the next poll that reads GitHub says it is back" grep -q "GitHub reachable again after " "$tmp/windows.log"
check "a setup that fails once is retried, logged" eval '[ "$(ran setupflake)" = "setup setup lint test " ] &&
  grep -q "^scripts/setup.sh: failed once; retried" "$wout/${flake:0:7}/summary.txt"'
check "and the job passes" has "$flake" "$W" "success	Passed in *"
check "a commit the queue dropped stops its job" has "$vanish" "$W" "error	Stopped: the queue dropped this commit	*"
check "before its next step (ran: $(ran vanish))" [ "$(ran vanish)" = "setup lint " ]
check "a commit merged on another result stops too" has "$merged" "$W" "error	Stopped: merged before this job finished	*"
check "oldest commit first" eval '[ "$(first pass)" -lt "$(first fail)" ] && [ "$(first fail)" -lt "$(first beat)" ] &&
  [ "$(first beat)" -lt "$(first slow)" ] && [ "$(first slow)" -lt "$(first vanish)" ] &&
  [ "$(first vanish)" -lt "$(first toolchain)" ] && [ "$(first toolchain)" -lt "$(first merged)" ]'
check "a job keeps target/" grep -q "^pass lint .* target=kept" "$FAKE/just.log"
check "a changed rust-toolchain.toml deletes it first" grep -q "^toolchain lint .* target=gone" "$FAKE/just.log"
check "and says so in the summary" grep -q "^upkeep: deleted target/ (rust-toolchain.toml changed)" "$wout/${toolchain:0:7}/summary.txt"
check "a job removes other untracked files" [ ! -e "$LOCAL_CI_HOME/queue-windows/mdoc/stray.txt" ]
check "output older than 7 days is deleted" [ ! -e "$wout/0ld0000" ]
check "newer output is kept" [ -d "$wout/recent0" ]
check "status.txt has the heartbeat" grep -q "^heartbeat: 20" "$wout/status.txt"
check "and nothing running or queued" eval 'grep -q "^running: nothing" "$wout/status.txt" && grep -q "^queue: empty" "$wout/status.txt"'
check "the lock is released" [ ! -e "$LOCAL_CI_HOME/queue-windows/state/lock" ]
check "status shows the runner's heartbeat" \
  grep -q "^heartbeat: " <<<"$(QUEUE_CI_JOB=windows QUEUE_CI_POLL=60 bash scripts/queue-runner.sh status)"
check "and that /ci is served and the page answers" \
  grep -qx "tailnet: /ci served, and https://ci.example/ci/queue-windows/status.txt answers" <<<"$(QUEUE_CI_JOB=windows QUEUE_CI_POLL=60 bash scripts/queue-runner.sh status)"
touch "$FAKE/page-down"
check "status fails when /ci is served but the page does not answer" \
  eval '! QUEUE_CI_JOB=windows QUEUE_CI_POLL=60 bash scripts/queue-runner.sh status >"$tmp/status.out" &&
  grep -q "^unhealthy: tailscale serves /ci, but https://ci.example/ci/queue-windows/status.txt does not answer" "$tmp/status.out"'
rm -f "$FAKE/page-down"
mkdir -p "$LOCAL_CI_HOME/out/other" && echo "ci/android queue runner on pc" >"$LOCAL_CI_HOME/out/other/status.txt"
check "or answers with another runner's status" \
  eval '! QUEUE_CI_JOB=windows QUEUE_CI_POLL=60 QUEUE_CI_URL=https://ci.example/ci/other bash scripts/queue-runner.sh status >"$tmp/status.out" &&
  grep -q "^unhealthy: tailscale serves /ci, but" "$tmp/status.out"'
touch "$FAKE/serve-off"
check "status fails while the tailnet does not serve /ci" \
  eval '! QUEUE_CI_JOB=windows QUEUE_CI_POLL=60 bash scripts/queue-runner.sh status >"$tmp/status.out" &&
  grep -q "^unhealthy: tailscale serve does not list /ci" "$tmp/status.out"'
check "and passes where there is no tailscale, saying so" \
  eval 'QUEUE_CI_JOB=windows QUEUE_CI_POLL=60 QUEUE_CI_TAILSCALE=$tmp/no-tailscale bash scripts/queue-runner.sh status >"$tmp/status.out" &&
  grep -q "^tailnet: no .*no-tailscale here; /ci not checked" "$tmp/status.out"'
rm -f "$FAKE/serve-off"
check "the job's refs are cleaned up" [ -z "$(git -C "$LOCAL_CI_HOME/queue-windows/mdoc" for-each-ref refs/queue-ci/c/)" ]

# --- the Android job, beside it ----------------------------------------------------

aout=$LOCAL_CI_HOME/out/queue-android
: >"$FAKE/just.log"
wposts=$(grep -c "	$W	" "$FAKE/statuses.tsv")
git clone --quiet "$FAKE_ORIGIN" "$LOCAL_CI_HOME/queue-android/mdoc"
mkdir -p "$LOCAL_CI_HOME/queue-android/mdoc/target"
echo cache >"$LOCAL_CI_HOME/queue-android/mdoc/target/kept"
QUEUE_CI_TARGET_MAX_GB=0 run_once android "$tmp/android.log"
check "an Android commit runs the Android job" [ "$(ran kotlin)" = "setup android " ]
check "which the runner asks for" grep -q "^kotlin android renders= job=always" "$FAKE/just.log"
check "and posts as ci/android in its own folder" \
  has "$kotlin" "$A" "success	Passed in *	https://ci.example/ci/queue-android/${kotlin:0:7}/"
check "with the Gradle reports" [ -f "$aout/${kotlin:0:7}/android-reports/index.html" ]
check "a failing emulator test is named from Gradle's XML" \
  has "$gradlefail" "$A" "failure	Failed in just android: android:io.github.lawkitt.mdoc.IdentitiesTest#aLongPressKeepsAllOfAnIdentity	*"
check "and listed in failed-tests.txt" \
  [ "$(cat "$aout/${gradlefail:0:7}/failed-tests.txt")" = "android:io.github.lawkitt.mdoc.IdentitiesTest#aLongPressKeepsAllOfAnIdentity" ]
check "with Gradle's XML copied to the job's folder" eval '[ -f "$aout/${gradlefail:0:7}/androidTest-results/connected/debug/TEST-mdoc-ci.xml" ] &&
  [ -f "$aout/${gradlefail:0:7}/test-results/testDebugUnitTest/TEST-parity.xml" ]'
check "and Gradle's last lines, plain, in the summary" \
  grep -qx "FAILURE: Build failed with an exception." "$aout/${gradlefail:0:7}/summary.txt"
check "a target/ over its size is deleted before a job" grep -q "^kotlin android .* target=gone" "$FAKE/just.log"
check "a desktop-only commit leaves ci/android not affected" has "$pass" "$A" "success	Not affected: no core or android change	"
check "a docs-only one passes it" has "$docs" "$A" "success	Docs only	"
check "as does a CI-only one" has "$gate" "$A" "success	Not affected: no core or android change	"
check "a core change runs it" has "$toolchain" "$A" "success	Passed in *"
check "the Android runner posts no ci/windows status" [ "$(grep -c "	$W	" "$FAKE/statuses.tsv")" = "$wposts" ]
check "nor builds the desktop commits" [ -z "$(ran pass)$(ran fail)" ]

# --- files a stopped job still holds ----------------------------------------------

# The checkout cannot clean at first (on Windows, a killed Gradle's open
# files; a git that fails `clean` once stands in): the job waits, retries
# once, and runs.
: >"$FAKE/just.log"
held=$(prefix=held/main/ qc 17 held src/lib.rs)
echo "17 User false" >>"$FAKE/prs.tsv"
hhome=$LOCAL_CI_HOME/queue-held
git clone --quiet "$FAKE_ORIGIN" "$hhome/mdoc"
mkdir -p "$hhome/mdoc/held" "$tmp/heldbin"
echo lock >"$hhome/mdoc/held/file.lock"
touch "$FAKE/held"
cat >"$tmp/heldbin/git" <<GIT
#!/usr/bin/env bash
if [ "\${1:-}" = clean ] && [ -e "$FAKE/held" ]; then
  rm -f "$FAKE/held"
  echo "warning: failed to remove held/file.lock: Invalid argument" >&2
  exit 1
fi
exec "$(command -v git)" "\$@"
GIT
chmod +x "$tmp/heldbin/git"
PATH="$tmp/heldbin:$PATH" QUEUE_CI_HOME=$hhome QUEUE_CI_OUT=$LOCAL_CI_HOME/out/queue-held QUEUE_CI_PREFIXES=held/main/ \
  QUEUE_CI_CLEAN_WAIT=1 run_once windows "$tmp/held.log"
check "a checkout that cannot clean at first is retried once, logged" \
  grep -q "^checkout: failed once; retried" "$LOCAL_CI_HOME/out/queue-held/${held:0:7}/summary.txt"
check "and the job runs" has "$held" "$W" "success	Passed in *"
check "with the folder gone" [ ! -e "$hhome/mdoc/held" ]

# --- the disk floor ----------------------------------------------------------------

# Its own prefix and commit, as the lone, held and update blocks have: a
# fresh runner that settled every commit above first took minutes on hosted
# Windows, past this block's wait (#614).
: >"$FAKE/just.log"
disk=$(prefix=disk/main/ qc 18 disk src/lib.rs)
echo "18 User false" >>"$FAKE/prs.tsv"
dhome=$LOCAL_CI_HOME/queue-disk
git clone --quiet "$FAKE_ORIGIN" "$dhome/mdoc"
QUEUE_CI_JOB=windows QUEUE_CI_HOME=$dhome QUEUE_CI_OUT=$LOCAL_CI_HOME/out/queue-disk QUEUE_CI_MIN_FREE_GB=100000000 \
  QUEUE_CI_PREFIXES=disk/main/ QUEUE_CI_FOREGROUND=1 bash scripts/queue-runner.sh run >"$tmp/disk.log" 2>&1 &
runner=$!
for _ in $(seq 1 60); do
  grep -q "Waiting for disk space" "$FAKE/statuses.tsv" && break
  sleep 1
done
# A few more polls: the status is posted once, not refreshed.
sleep 4
kill "$runner" 2>/dev/null || true
wait "$runner" 2>/dev/null || true
check "below the disk floor no job starts" [ ! -s "$FAKE/just.log" ]
check "and the oldest commit's status says so, once" \
  [ "$(grep -c "^$disk	$W	pending	Waiting for disk space: [0-9]* GB free; a job starts at 100000000 GB	https://ci.example/ci/queue-windows/status.txt" "$FAKE/statuses.tsv")" = 1 ]
check "as does status.txt" grep -q "^waiting: .* GB free; a job starts at 100000000 GB" "$LOCAL_CI_HOME/out/queue-disk/status.txt"
check "a stopped runner releases its lock" [ ! -e "$dhome/state/lock" ]

# --- a lone commit whose PR cannot be read at first ------------------------------

# Nothing else keeps the runner polling: `once` must not take the queue for
# empty while the commit waits to be settled again.
: >"$FAKE/just.log"
lone=$(prefix=lone/main/ qc 16 lone src/lib.rs)
echo "16 User false" >>"$FAKE/prs.tsv"
echo 2 >"$FAKE/pr-down-16"
lhome=$LOCAL_CI_HOME/queue-lone
git clone --quiet "$FAKE_ORIGIN" "$lhome/mdoc"
QUEUE_CI_HOME=$lhome QUEUE_CI_OUT=$LOCAL_CI_HOME/out/queue-lone QUEUE_CI_PREFIXES=lone/main/ \
  run_once windows "$tmp/lone.log"
check "a lone commit whose PR read fails is built once it can be read" has "$lone" "$W" "success	Passed in *"
check "after the runner said it was retrying" grep -q "${lone:0:7} .*could not read #16" "$tmp/lone.log"

# --- self-update --------------------------------------------------------------------

# A newer copy of the runner reaches main while a job runs: the installed
# copy finishes the job, then updates itself and restarts.
uhome=$LOCAL_CI_HOME/queue-update
git clone --quiet "$FAKE_ORIGIN" "$uhome/mdoc"
mkdir -p "$uhome/runner/scripts"
cp scripts/queue-runner.sh scripts/lib.sh "$uhome/runner/scripts/"
git_ fetch --quiet "$FAKE_ORIGIN" main
git_ checkout --quiet -B next-main FETCH_HEAD
echo '# a newer copy' >>"$tmp/work/scripts/queue-runner.sh"
git_ commit --quiet -am "a newer runner"
git_ rev-parse HEAD >"$FAKE/next-main"
git_ push --quiet "$FAKE_ORIGIN" HEAD:refs/heads/hold/next-main
bump=$(prefix=upd/main/ qc 13 bump src/lib.rs)
echo "13 User false" >>"$FAKE/prs.tsv"
# Its own 2-minute limit: on hosted Windows, Git Bash took 16 s for the
# checkout and lint alone (#525's first main run timed out there).
QUEUE_CI_HOME=$uhome QUEUE_CI_OUT=$LOCAL_CI_HOME/out/queue-update QUEUE_CI_PREFIXES=upd/main/ QUEUE_CI_TIMEOUT=120 \
  run_once windows "$tmp/update.log" "$uhome/runner/scripts/queue-runner.sh"
line_of() { grep -n "$1" "$tmp/update.log" | head -1 | cut -d: -f1; }
check "the job that brought it finishes" has "$bump" "$W" "success	Passed in *"
check "then the idle runner updates itself" grep -q "updated this runner's queue-runner.sh from main@" "$tmp/update.log"
check "after the result" [ "$(line_of "success $W on ${bump:0:7}")" -lt "$(line_of "updated this runner")" ]
check "to main's copy" cmp -s "$uhome/runner/scripts/queue-runner.sh" <(git_ show next-main:scripts/queue-runner.sh)
check "and restarts as it" [ "$(grep -c "runner started" "$tmp/update.log")" = 2 ]
check "which does not run the job again" [ "$(grep -c "^bump lint" "$FAKE/just.log")" = 1 ]
# Line endings aside, a copy that matches main's stays.
(
  # Its own count, which the parent adds: it once exited with the parent's,
  # which was then counted twice.
  failures=0
  export QUEUE_RUNNER_SOURCED=1
  # shellcheck source=queue-runner.sh
  source scripts/queue-runner.sh
  # shellcheck disable=SC2034 # read by outdated_files
  checkout=$uhome/mdoc runner_dir=$tmp/crlf
  mkdir -p "$runner_dir"
  git -C "$checkout" fetch --quiet origin "+refs/heads/main:refs/queue-ci/main"
  git -C "$checkout" show refs/queue-ci/main:scripts/queue-runner.sh | sed 's/$/\r/' >"$runner_dir/queue-runner.sh"
  git -C "$checkout" show refs/queue-ci/main:scripts/lib.sh >"$runner_dir/lib.sh"
  check "a copy with CRLF line endings is not outdated" [ -z "$(outdated_files)" ]
  echo '# changed' >>"$runner_dir/lib.sh"
  check "a changed lib.sh is" [ "$(outdated_files)" = lib.sh ]
  exit "$failures"
) || failures=$((failures + $?))

if [ "$failures" -gt 0 ]; then
  for f in windows android disk lone held update; do
    echo "--- $f runner log" >&2
    cat "$tmp/$f.log" >&2 2>/dev/null || true
  done
  echo "--- statuses" >&2
  cat "$FAKE/statuses.tsv" >&2
  echo "--- just" >&2
  cat "$FAKE/just.log" >&2
  die "$failures queue runner check(s) failed"
fi
echo "queue runner: all checks passed"
