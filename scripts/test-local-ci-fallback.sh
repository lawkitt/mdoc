#!/usr/bin/env bash
# Usage: scripts/test-local-ci-fallback.sh
#
# The hosted fallback for local CI (scripts/local-ci-fallback.sh) on canned
# GitHub answers: fake PR list, check runs, dispatch and status posts; no
# network. Needs jq, as the fallback does.
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
if ! command -v jq >/dev/null; then
  echo "skip: jq not found (the fallback runs on ubuntu-latest, which has it)"
  exit 0
fi
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

export FALLBACK_SOURCED=1 LOCAL_CI_REPO=example/mdoc
# shellcheck source=local-ci-fallback.sh
source scripts/local-ci-fallback.sh

check "the stand-ins are ci.yml's shipped runners" \
  grep -q "runner: &shipped \[$(hosted_runner local/macos), $(hosted_runner local/windows)\]" .github/workflows/ci.yml

# --- canned GitHub --------------------------------------------------------------

# Now is 16:00; "m20" is 20 minutes before.
at() { printf '2026-10-31T%s:00Z' "$1"; }
m5=$(at 15:55) m10=$(at 15:50) m20=$(at 15:40) m30=$(at 15:30) m60=$(at 15:00)
m70=$(at 14:50) old=$(at 09:00)

st() { # context state description created
  printf '{"context":"%s","state":"%s","description":"%s","createdAt":"%s"}' "$@"
}
# number draft cross author lint-started statuses... Without a status of
# their own, local/ios and local/android are "Not affected"; "-<context>"
# leaves that context without one.
pr_node() {
  local n=$1 draft=$2 cross=$3 author=$4 lint=$5 statuses ctx
  shift 5
  local -a all=()
  for ctx in local/ios local/android; do
    [[ " $* " == *"\"context\":\"$ctx\""* || " $* " == *" -$ctx "* ]] ||
      all+=("$(st "$ctx" SUCCESS "Not affected: no core or ${ctx#local/} change (main@abc)" "$old")")
  done
  for ctx in "$@"; do [[ $ctx == -* ]] || all+=("$ctx"); done
  statuses=$(
    IFS=,
    echo "${all[*]}"
  )
  printf '{"number":%s,"isDraft":%s,"isCrossRepository":%s,"headRefName":"b%s","headRefOid":"head%s000000",' \
    "$n" "$draft" "$cross" "$n" "$n"
  printf '"author":{"__typename":"%s"},"commits":{"nodes":[{"commit":{"committedDate":"%s",' "$author" "$old"
  printf '"status":{"contexts":[%s]},' "$statuses"
  printf '"statusCheckRollup":{"contexts":{"nodes":[{"__typename":"CheckRun","name":"lint (ubuntu)","startedAt":"%s"}]}}}}]}}' "$lint"
}
fb() { echo "Hosted fallback: hosted CI on head$1 (attempt $2); stuck"; }
nodes=(
  # 1: the Mac never posted: claimed; Windows passed: left alone.
  "$(pr_node 1 false false User "$m20" "$(st local/windows SUCCESS Passed "$m10")")"
  # 2: a Mac queue of 30 min is not stuck; a Windows capacity wait of 20 min is.
  "$(pr_node 2 false false User "$m60" "$(st local/macos PENDING Queued "$m30")" \
    "$(st local/windows PENDING "Queued; waiting for disk space (starts at 40 GB free)" "$m20")")"
  # 3: a step running 60 min means the runner stopped; 10 min does not.
  "$(pr_node 3 false false User "$m60" "$(st local/macos PENDING "Running just test on main@abc" "$m60")" \
    "$(st local/windows PENDING "Running just lint on main@abc" "$m10")")"
  # 4-6: a draft, a fork and a bot: never touched.
  "$(pr_node 4 true false User "$m60")"
  "$(pr_node 5 false true User "$m60")"
  "$(pr_node 6 false false Bot "$m60")"
  # 7: hosted runs finished: Mac passed (after an older failure), Windows failed.
  "$(pr_node 7 false false User "$m60" "$(st local/macos PENDING "$(fb 7 1)" "$m10")" \
    "$(st local/windows PENDING "$(fb 7 1)" "$m10")")"
  # 8: no hosted run after 30 min: the Mac retries; Windows gives up at attempt 3.
  "$(pr_node 8 false false User "$m60" "$(st local/macos PENDING "$(fb 8 1)" "$m30")" \
    "$(st local/windows PENDING "$(fb 8 3)" "$m30")")"
  # 9: a hosted run in progress for the Mac also serves a newly stuck Windows.
  "$(pr_node 9 false false User "$m20" "$(st local/macos PENDING "$(fb 9 1)" "$m10")")"
  # 10: a fresh head is not stuck; a cancelled hosted run is retried.
  "$(pr_node 10 false false User "$m5" "$(st local/windows PENDING "$(fb 10 1)" "$m30")")"
  # 11: a runner took the head back from the fallback: left alone.
  "$(pr_node 11 false false User "$m60" "$(st local/macos PENDING "Running just lint on main@abc" "$m5")" \
    "$(st local/windows SUCCESS "Passed on main@abc" "$m5")")"
  # 12: hosted passed, but the runner posted a failure after this pass read
  # the PR list: the runner's result stays.
  "$(pr_node 12 false false User "$m60" "$(st local/macos PENDING "$(fb 12 1)" "$m10")" \
    "$(st local/windows SUCCESS Passed "$m10")")"
  # 13: the Mac's hosted jobs were cancelled, the Windows ones still run: the
  # retry waits for that run without spending an attempt.
  "$(pr_node 13 false false User "$m60" "$(st local/macos PENDING "$(fb 13 1)" "$m30")" \
    "$(st local/windows PENDING "$(fb 13 1)" "$m10")")"
  # 14: hosted jobs queued for 70 min are retried; 15: queued for 30 are not.
  "$(pr_node 14 false false User "$m60" "$(st local/macos PENDING "$(fb 14 1)" "$m70")" \
    "$(st local/windows SUCCESS Passed "$m10")")"
  "$(pr_node 15 false false User "$m60" "$(st local/macos SUCCESS Passed "$m10")" \
    "$(st local/windows PENDING "$(fb 15 1)" "$m30")")"
  # 16: no local/ios after 20 min, local/android queued 50: both posted
  # untested success (no hosted mobile job yet), with no dispatch.
  "$(pr_node 16 false false User "$m20" "$(st local/macos SUCCESS Passed "$m10")" \
    "$(st local/windows SUCCESS Passed "$m10")" -local/ios "$(st local/android PENDING Queued "$(at 15:10)")")"
  # 17: local/ios queued 30 min is not stuck; a mobile step running 60 min is.
  "$(pr_node 17 false false User "$m60" "$(st local/macos SUCCESS Passed "$m10")" \
    "$(st local/windows SUCCESS Passed "$m10")" "$(st local/ios PENDING Queued "$m30")" \
    "$(st local/android PENDING "Running just android on main@abc" "$m60")")"
)
printf '{"data":{"repository":{"pullRequests":{"nodes":[%s]}}}}' "$(
  IFS=,
  echo "${nodes[*]}"
)" >"$tmp/prs.json"

run() { # id name status conclusion
  printf '{"id":%s,"name":"%s","status":"%s","conclusion":%s,"html_url":"https://gh/run/%s"}' \
    "$1" "$2" "$3" "$4" "$1"
}
runs() { # sha runs...
  local sha=$1
  shift
  printf '{"check_runs":[%s]}' "$(
    IFS=,
    echo "$*"
  )" >"$tmp/checks-$sha.json"
}
runs head7000000 \
  "$(run 1 "test (macos-26)" completed '"failure"')" \
  "$(run 5 "test (macos-26)" completed '"success"')" \
  "$(run 6 "clippy (macos-26)" completed '"success"')" \
  "$(run 7 "test (windows-latest)" completed '"failure"')" \
  "$(run 8 "clippy (windows-latest)" completed '"success"')"
runs head8000000
runs head9000000 \
  "$(run 9 "test (macos-26)" in_progress null)" \
  "$(run 10 "clippy (macos-26)" completed '"success"')" \
  "$(run 11 "test (windows-latest)" in_progress null)" \
  "$(run 12 "clippy (windows-latest)" in_progress null)"
runs head12000000 \
  "$(run 15 "test (macos-26)" completed '"success"')" \
  "$(run 16 "clippy (macos-26)" completed '"success"')"
printf '{"statuses":[{"context":"local/macos","state":"failure","description":"Failed in just test on main@abc"}]}' \
  >"$tmp/status-head12000000.json"
runs head13000000 \
  "$(run 17 "test (macos-26)" completed '"cancelled"')" \
  "$(run 18 "clippy (macos-26)" completed '"cancelled"')" \
  "$(run 19 "test (windows-latest)" in_progress null)" \
  "$(run 20 "clippy (windows-latest)" completed '"success"')"
runs head14000000 \
  "$(run 21 "test (macos-26)" queued null)" \
  "$(run 22 "clippy (macos-26)" queued null)"
runs head15000000 \
  "$(run 23 "test (windows-latest)" queued null)" \
  "$(run 24 "clippy (windows-latest)" queued null)"
runs head10000000 \
  "$(run 13 "test (windows-latest)" completed '"cancelled"')" \
  "$(run 14 "clippy (windows-latest)" completed '"cancelled"')"

# shellcheck disable=SC2317,SC2329 # the fakes replace the script's functions
fetch_prs() { cat "$tmp/prs.json"; }
# shellcheck disable=SC2317,SC2329
fetch_check_runs() { cat "$tmp/checks-$1.json" 2>/dev/null || echo '{"check_runs":[]}'; }
# The live status: the PR list's, unless a runner posted since.
# shellcheck disable=SC2317,SC2329
fetch_status() {
  if [ -f "$tmp/status-$1.json" ]; then
    cat "$tmp/status-$1.json"
  else
    jq --arg sha "$1" '.data.repository.pullRequests.nodes[] | select(.headRefOid == $sha)
      | {statuses: [.commits.nodes[0].commit.status.contexts[]
          | {context, state: (.state | ascii_downcase), description}]}' "$tmp/prs.json"
  fi
}
# shellcheck disable=SC2317,SC2329
dispatch() { echo "$1" >>"$tmp/dispatched"; }
# shellcheck disable=SC2317,SC2329
post() { printf '%s\t%s\t%s\t%s\t%s\n' "$@" >>"$tmp/posted"; }
# shellcheck disable=SC2317,SC2329
now() { jq -n '"2026-10-31T16:00:00Z" | fromdateiso8601'; }
: >"$tmp/dispatched"
: >"$tmp/posted"

tick >"$tmp/log" 2>&1 || {
  cat "$tmp/log" >&2
  die "tick failed"
}

# --- checks ---------------------------------------------------------------------

posted() { # sha context: "state<TAB>description<TAB>url" of its post
  awk -F'\t' -v h="$1" -v c="$2" -v OFS='\t' '$1 == h && $2 == c { print $3, $4, $5 }' "$tmp/posted"
}
# shellcheck disable=SC2053 # $3 is a glob
has() { [[ $(posted "$1" "$2") == $3 ]]; }
none() { [ -z "$(posted "$1" "$2")" ]; }
dispatched() { grep -qx "$1" "$tmp/dispatched"; }

check "dispatches once per PR that needs it" \
  [ "$(sort "$tmp/dispatched" | tr '\n' ' ')" = "b1 b10 b14 b2 b3 b8 " ]
check "a head with no runner status is claimed" \
  has head1000000 local/macos "pending	Hosted fallback: hosted CI on head100 (attempt 1); no runner status after 20 min	https://github.com/example/mdoc/actions/workflows/ci.yml?query=branch%3Ab1"
check "a finished context is left alone" none head1000000 local/windows
check "a 30-min queue is not stuck" none head2000000 local/macos
check "a 20-min capacity wait is claimed" has head2000000 local/windows "pending	Hosted fallback: * (attempt 1); \"Queued; waiting for disk*for 20 min	*"
check "a 60-min step is claimed" has head3000000 local/macos "pending	*\"Running just test on main@abc\" for 60 min	*"
check "a 10-min step is not" none head3000000 local/windows
for n in 4 5 6; do
  check "drafts, forks and bots are never touched (#$n)" eval "! grep -q '^head${n}000000' \"\$tmp/posted\" && ! dispatched b$n"
done
check "a hosted pass posts success, the newest run winning, and names the missing mobile step" \
  has head7000000 local/macos "success	Hosted fallback: clippy and test (macos-26) passed; just mobile not run	https://gh/run/5"
check "a hosted failure posts failure naming the job" \
  has head7000000 local/windows "failure	Hosted fallback: test (windows-latest) failed	https://gh/run/7"
check "a PR with results is not dispatched again" eval '! dispatched b7'
check "no hosted run in 25 min retries" has head8000000 local/macos "pending	Hosted fallback: * (attempt 2); hosted run missing after 30 min	*"
check "the third attempt gives up with an error" has head8000000 local/windows "error	Hosted fallback: no hosted result after 3 dispatches*"
check "a running hosted run is left alone" none head9000000 local/macos
check "a newly stuck context joins a running hosted run" has head9000000 local/windows "pending	Hosted fallback: * (attempt 1); no runner status after 20 min; joins the running hosted run	*"
check "joining a running hosted run does not dispatch" eval '! dispatched b9'
check "a fresh head is not claimed" none head10000000 local/macos
check "a cancelled hosted run is retried" has head10000000 local/windows "pending	* (attempt 2); hosted run missing after 30 min	*"
check "a head a runner took back is left alone" eval "! grep -q '^head11000000' \"\$tmp/posted\""
check "a runner result posted during the pass is never overwritten" none head12000000 local/macos
check "the race is logged" grep -q "head120 local/macos: changed since read; left alone" "$tmp/log"
check "a retry waits for the head's running hosted run" none head13000000 local/macos
check "and spends no attempt or dispatch" eval '! dispatched b13'
check "hosted jobs queued 70 min are retried" has head14000000 local/macos "pending	* (attempt 2); hosted run queued after 70 min	*"
check "hosted jobs queued 30 min are left to start" none head15000000 local/windows
adr=https://github.com/example/mdoc/blob/main/docs/adr/0091-mobile-ci-and-releases.md
check "a stuck local/ios is posted success, saying plainly it was NOT tested (temporary)" \
  has head16000000 local/ios "success	Hosted fallback: NOT tested; no hosted iOS job yet (ADR 0091 item 5); no runner status after 20 min	$adr"
check "so is a stuck local/android" \
  has head16000000 local/android "success	Hosted fallback: NOT tested; no hosted Android job yet (ADR 0091 item 5); \"Queued\" for 50 min	$adr"
check "with no hosted run dispatched for them" eval '! dispatched b16 && ! dispatched b17'
check "a 30-min mobile queue is not stuck" none head17000000 local/ios
check "a mobile step running 60 min is" has head17000000 local/android "success	Hosted fallback: NOT tested; no hosted Android job yet*\"Running just android on main@abc\" for 60 min	*"
check "a mobile status the runner settled is left alone" none head1000000 local/ios
check "the mobile contexts have no hosted runner yet" eval '! hosted_runner local/ios && ! hosted_runner local/android'
check "untested_job names them" [ "$(untested_job local/ios), $(untested_job local/android)" = "iOS, Android" ]

# Each stuck rule on its own, at its edge.
# shellcheck disable=SC2016 # the eval strings expand when eval runs them
{
  check "no status: 14 min is not stuck" eval '! stuck_reason NONE "" 0 $((1000 - 14 * 60)) 1000'
  check "no status: unknown head time is not stuck" eval '! stuck_reason NONE "" 0 0 100000'
  check "queued: 45 min is stuck" eval 'stuck_reason PENDING Queued $((10000 - 45 * 60)) 0 10000 >/dev/null'
  check "queued: 44 min is not" eval '! stuck_reason PENDING Queued $((10000 - 44 * 60)) 0 10000'
  check "running: 49 min is not stuck" eval '! stuck_reason PENDING "Running just test" $((10000 - 49 * 60)) 0 10000'
  check "the fallback's own status is never stuck" eval '! stuck_reason PENDING "Hosted fallback: x" 0 0 100000'
  check "final states are never stuck" eval '! stuck_reason FAILURE x 0 0 100000'
  check "attempt is read from the description" [ "$(attempt_of "$(fb 1 2)")" = 2 ]
  check "attempt defaults to 1" [ "$(attempt_of "Hosted fallback: x")" = 1 ]
}

if [ "$failures" -gt 0 ]; then
  echo "--- log" >&2
  cat "$tmp/log" >&2
  echo "--- posted" >&2
  cat "$tmp/posted" >&2
  die "$failures local CI fallback check(s) failed"
fi
echo "local CI fallback: all checks passed"
