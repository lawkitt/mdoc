#!/usr/bin/env bash
# Usage: scripts/test-local-ci.sh
#
# The local CI runner (scripts/local-ci.sh) and the area table
# (scripts/ci-areas.txt) against a local origin with refs/pull/N/merge refs,
# a fake gh that keeps commit statuses in a file and a fake just; no network,
# builds or real statuses.
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

not() { ! "$@"; }

# --- the area table, ci.yml and the steps ---------------------------------------

(
  export LOCAL_CI_SOURCED=1
  # shellcheck source=local-ci.sh
  source scripts/local-ci.sh
  # A Windows checkout may have it with CRLF.
  table=$(tr -d '\r' <scripts/ci-areas.txt)

  # ci.yml's paths-ignore is the table's `none` lines, word for word.
  # The first paths-ignore list in the YAML on stdin, one pattern per line.
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
  ignored=$(tr -d '\r' <.github/workflows/ci.yml | paths_ignore)
  # The lines above the first none line are exceptions to it (Markdown a
  # test reads); paths-ignore lists them after the none lines with a `!`.
  none=$(grep -vE '^[[:space:]]*(#|$)' <<<"$table" | awk '
    $1 == "none" { seen = 1; print $2; next }
    !seen { except[++n] = $2 }
    END { for (i = 1; i <= n; i++) print "!" except[i] }')
  check "ci.yml's paths-ignore list is read" [ -n "$ignored" ]
  check "ci.yml's paths-ignore is the table's none lines, then its exceptions, in order" [ "$ignored" = "$none" ]
  check "the exceptions are under tests/fixtures/, so a README under apps/ is docs" \
    [ -z "$(grep -vE '^[[:space:]]*(#|$)' <<<"$table" | awk '$1 == "none" { exit } { print $2 }' | grep -v '^tests/fixtures/')" ]
  check "the none lines are one block" \
    [ "$(grep -vE '^[[:space:]]*(#|$)' <<<"$table" | awk '$1 == "none" { inside = 1; next } inside { after = 1 } after && $1 == "none" { bad = 1 } END { print bad + 0 }')" = 0 ]
  check "every line names known areas and one glob" \
    [ -z "$(grep -vE '^[[:space:]]*(#|$)' <<<"$table" | grep -vE '^(none|core|desktop|ios|android)(,(desktop|ios|android))*[[:space:]]+[^[:space:]]+$')" ]
  # The desktop jobs follow the table through ci.yml's areas job.
  check "ci.yml's areas job reads the table" grep -q 'run: scripts/ci-areas.sh --github' .github/workflows/ci.yml
  check "hosted clippy and test wait for it" \
    [ "$(grep -cE '^    needs: areas$' .github/workflows/ci.yml)" = 2 ]
  check "and run only for the desktop area" grep -q "if: &desktop \${{ needs.areas.outputs.desktop == 'true' }}" .github/workflows/ci.yml

  # Routing: a file takes the first matching line's areas, desktop by default.
  areas_of() { printf '%s\0' "$@" | changed_areas "$table"; }
  routes() { # expected files...
    local want=$1
    shift
    check "$* -> ${want:-no area}" [ "$(areas_of "$@")" = "$want" ]
  }
  routes ios apps/ios/mdoc/App/MdocApp.swift
  routes ios apps/ios/mdoc.xcodeproj/project.pbxproj scripts/build-ios-core.sh scripts/ios-survey.sh scripts/ios-scan-pages.swift
  routes android apps/android/app/build.gradle.kts
  routes android scripts/build-android-core.sh scripts/android-emulator.sh scripts/android-package-xml.py scripts/install-android-sdk.sh
  routes "core desktop ios android" crates/mdoc-ffi/src/lib.rs
  routes "core desktop ios android" Cargo.lock
  routes "core desktop ios android" scripts/lib.sh
  routes "core desktop ios android" justfile apps/ios/mdoc/App/MdocApp.swift
  routes "core desktop ios android" scripts/ci-areas.txt
  routes desktop src/main.rs
  routes desktop tests/render.rs vendor/gpui-pre-macos/src/lib.rs .github/workflows/ci.yml
  routes desktop scripts/release.sh scripts/local-ci.sh scripts/test-local-ci.sh
  routes "desktop ios" scripts/mobile-check.sh
  routes "desktop android" scripts/install-android-ndk.sh
  routes "desktop ios" src/main.rs apps/ios/mdoc/App/MdocApp.swift
  routes "ios android" apps/ios/a.swift apps/android/b.kt
  routes "core desktop ios android" tests/fixtures/recognition/en/claim/x.md
  routes "core desktop ios android" tests/fixtures/recognition/en/claim/x.model.json
  routes "" tests/fixtures/pseudonymization/README.md
  routes "" README.md docs/adr/0091-mobile-ci-and-releases.md LICENSE
  routes "" apps/ios/README.md crates/mdoc-ffi/README.md changes/ios/x.md
  routes "ios" apps/ios/README.md apps/ios/a.swift
  check "no files runs every area" [ "$(changed_areas "$table" </dev/null)" = "core desktop ios android" ]
  check "a table with CRLF line endings routes the same" \
    [ "$(printf 'apps/ios/a.swift\0' | changed_areas "${table//$'\n'/$'\r\n'}")" = ios ]
  check "docs.rs is not under docs/" [ "$(areas_of docs.rs)" = desktop ]
  # shellcheck disable=SC2317,SC2329 # called through check
  matches() { [[ $1 =~ $(glob_regex "$2") ]]; }
  check "* does not cross directories" not matches scripts/x/ios-a.sh "scripts/ios-*.sh"
  check "a dot is literal" not matches build/appiconxsvg build/appicon.svg
  check "** crosses directories" matches docs/a/b/c.png "docs/**"
  check "**/ matches at the root and below" eval 'matches b.md "**/*.md" && matches a/b.md "**/*.md"'
  check "affects reads the list" eval 'affects ios "desktop ios" && ! affects android "desktop ios"'
  # The CLI's GitHub mode: anything but a PR or a known push runs every area.
  every="areas=core desktop ios android"$'\n'"desktop=true"
  check "a manual run runs every area" [ "$(EVENT=workflow_dispatch bash scripts/ci-areas.sh --github)" = "$every" ]
  check "a release call runs every area" [ "$(EVENT=push RELEASE_CALL=true BEFORE=1234567890123456789012345678901234567890 bash scripts/ci-areas.sh --github)" = "$every" ]
  check "a new branch's push runs every area" [ "$(EVENT=push BEFORE=0000000000000000000000000000000000000000 bash scripts/ci-areas.sh --github)" = "$every" ]

  # The statuses: which area each stands for, and what a skipped one says.
  # shellcheck disable=SC2034 # read by area_of, steps_of and queue
  context=local/macos mobile_context=local/ios mobile_area=ios steps="setup lint test ffi detect" mobile_steps="setup ios"
  check "local/macos is desktop" [ "$(area_of local/macos)" = desktop ]
  check "local/ios is ios" [ "$(area_of local/ios)" = ios ]
  check "the iOS job's steps" [ "$(steps_of local/ios)" = "setup ios" ]
  check "a Swift-only PR leaves local/macos not affected" \
    [ "$(settled_text local/macos ios main@abc)" = "Not affected: no core or desktop change (main@abc)" ]
  check "and runs local/ios" eval '! settled_text local/ios ios main@abc'
  check "a core PR runs both" eval '! settled_text local/macos "core desktop ios android" x && ! settled_text local/ios "core desktop ios android" x'
  check "a docs-only PR passes both" [ "$(settled_text local/ios "" main@abc)" = "Docs only, not tested (main@abc)" ]

  # Step limits: the mobile cargo check shares lint's; the mobile jobs have their own.
  # shellcheck disable=SC2034 # read by timeout_for
  lint_s=1200 test_s=1800 mobile_s=2400 android_s=2400
  check "lint keeps its limit" [ "$(timeout_for lint)" = 1200 ]
  check "setup shares lint's limit" [ "$(timeout_for setup)" = 1200 ]
  check "the mobile cargo check shares lint's limit" [ "$(timeout_for ffi)" = 1200 ]
  check "the iOS job has its own limit" [ "$(timeout_for ios)" = 2400 ]
  check "the Android job has its own limit" [ "$(timeout_for android)" = 2400 ]
  check "test keeps its limit" [ "$(timeout_for test)" = 1800 ]
  check "the steps read as their commands" [ "$(label ffi), $(label ios), $(label android), $(label detect)" = "just mobile --ffi, just mobile --ios, just android, just detection-gate" ]
  check "the detection gate has test's limit" [ "$(timeout_for detect)" = 1800 ]

  # The mobile steps follow the merge's main.
  repo=$(mktemp -d)
  g() { git -C "$repo" -c user.name=t -c user.email=t@example.com "$@"; }
  g init --quiet -b main
  echo a >"$repo/a" && g add a && g commit --quiet -m old-main
  old_main=$(g rev-parse HEAD)
  mkdir "$repo/scripts" && : >"$repo/scripts/mobile-check.sh" && : >"$repo/scripts/android-check.sh" && : >"$repo/scripts/detection-gate.sh"
  g add scripts && g commit --quiet -m new-main
  new_main=$(g rev-parse HEAD)
  # A PR merged onto the old main: no check to run.
  g checkout --quiet -b pr-old "$old_main" && echo b >"$repo/b" && g add b && g commit --quiet -m pr
  g checkout --quiet --detach "$old_main" && g merge --quiet --no-ff -m merge pr-old
  # shellcheck disable=SC2317,SC2329 # called through check
  in_repo() { (cd "$repo" && mobile_check_expected); }
  # shellcheck disable=SC2317,SC2329 # called through check
  android_in_repo() { (cd "$repo" && android_check_expected); }
  # shellcheck disable=SC2317,SC2329 # called through check
  gate_in_repo() { (cd "$repo" && detection_gate_expected); }
  check "a merge ref on a main without the detection gate skips it" eval '! gate_in_repo'
  check "a merge ref on a main without the check skips it" eval '! in_repo'
  check "a merge ref on a main without the Android job skips it" eval '! android_in_repo'
  # A PR that deletes the script, merged onto the new main: the check still runs (and fails).
  g checkout --quiet -b pr-new "$new_main" && g rm --quiet scripts/mobile-check.sh && g commit --quiet -m drop
  g checkout --quiet --detach "$new_main" && g merge --quiet --no-ff -m merge pr-new
  check "a PR that removes the check still runs it" in_repo
  check "and the detection gate too" gate_in_repo
  # A commit without the table runs every area.
  # shellcheck disable=SC2034 # read by commit_areas
  checkout=$repo
  check "a commit without the area table runs every area" [ "$(commit_areas HEAD HEAD^1..HEAD)" = "core desktop ios android" ]
  # The installed copy against main's: lib.sh matches (CRLF aside), local-ci.sh does not.
  # shellcheck disable=SC2034 # read by check_copy
  runner_dir=$repo/runner
  mkdir -p "$runner_dir"
  printf 'old\n' >"$runner_dir/local-ci.sh"
  printf 'lib\r\n' >"$runner_dir/lib.sh"
  printf 'new\n' >"$repo/scripts/local-ci.sh"
  printf 'lib\n' >"$repo/scripts/lib.sh"
  g add scripts && g commit --quiet -m runner && g update-ref refs/local-ci/main HEAD
  outdated=''
  check "a runner copy behind main's is logged" grep -q "warning: this runner's local-ci.sh differ from main's" <<<"$(check_copy)"
  check_copy >/dev/null
  check "and named for status.txt, line endings aside" [ "$outdated" = local-ci.sh ]
  cp "$repo/scripts/local-ci.sh" "$runner_dir/local-ci.sh"
  check_copy >/dev/null
  check "a copy that matches main's is not outdated" [ -z "$outdated" ]
  # Names git would quote still route: read NUL-separated.
  printf '%s\n' "$table" >"$repo/scripts/ci-areas.txt"
  g add scripts && g commit --quiet -m table
  mkdir -p "$repo/apps/ios" "$repo/apps/android"
  echo a >"$repo/apps/ios/Сканер.swift"
  echo b >"$repo/apps/android/a \"b\".kt"
  g add apps && g commit --quiet -m names
  check "non-ASCII and quoted file names keep their areas" [ "$(commit_areas HEAD HEAD^1..HEAD)" = "ios android" ]
  # main's queue: a stop at any point repeats a commit, never skips it.
  g remote add origin "$repo"
  g update-ref refs/heads/main HEAD
  echo c >"$repo/src.rs" && echo c >>"$repo/apps/ios/Сканер.swift"
  g add -A && g commit --quiet -m both
  both=$(g rev-parse HEAD)
  echo d >"$repo/notes.md" && g add -A && g commit --quiet -m docs
  docs_commit=$(g rev-parse HEAD)
  g update-ref refs/heads/main HEAD
  # shellcheck disable=SC2034 # read by queue_main
  state=$(mktemp -d) main_max=10 url=''
  g rev-parse HEAD~2 >"$state/main-last"
  # shellcheck disable=SC2317,SC2329 # stops the runner at the first Queued post
  post() { [ "$3" != pending ] || exit 0; }
  (queue_main) >/dev/null 2>&1 || true
  check "a stop after queueing keeps both jobs" [ "$(cut -f2 "$state/main-queue" | tr '\n' ' ')" = "local/macos local/ios " ]
  check "and moves the cursor" [ "$(cat "$state/main-last")" = "$both" ]
  rm -f "$state/main-queue"
  # shellcheck disable=SC2317,SC2329 # stops it at the first settled post
  post() { [ "$3" != success ] || exit 0; }
  (queue_main) >/dev/null 2>&1 || true
  check "a stop while settling leaves the cursor, so the commit repeats" [ "$(cat "$state/main-last")" = "$both" ]
  check "with nothing queued for it" [ ! -s "$state/main-queue" ]
  # shellcheck disable=SC2317,SC2329
  post() { post_ok=true; }
  queue_main >/dev/null 2>&1 || true
  check "a full pass settles it" [ "$(cat "$state/main-last")" = "$docs_commit" ]
  # A main result GitHub did not take keeps the job queued.
  printf '%s\tlocal/ios\n' "$both" >"$state/main-queue"
  # shellcheck disable=SC2317,SC2329
  post() { post_ok=false; }
  # shellcheck disable=SC2034 # read by finish
  run_kind=main run_head=$both run_ctx=local/ios run_url='' run_dir=$state
  finish failure "Failed in just mobile --ios on main@abc" >/dev/null
  check "a main result that failed to post stays queued" grep -q "local/ios" "$state/main-queue"
  # shellcheck disable=SC2317,SC2329
  post() { post_ok=true; }
  # shellcheck disable=SC2034
  run_kind=main run_head=$both run_ctx=local/ios run_url='' run_dir=$state
  finish failure "Failed in just mobile --ios on main@abc" >/dev/null
  check "one that posted leaves the queue" [ ! -s "$state/main-queue" ]
  rm -rf "${state:?}"
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
  # Queued rows get the current text; running, fallback and final rows do not.
  posts=$(mktemp)
  # shellcheck disable=SC2317,SC2329
  post() { printf '%s\t%s\t%s\n' "$2" "$1" "$4" >>"$posts"; }
  # shellcheck disable=SC2034
  os=macos max_pressure=1 fake_pressure=2 run_pr=5 run_ctx=local/macos url=''
  capacity_ok >/dev/null || true
  line() { # number context state [time description]
    printf '%s\tfalse\tfalse\tUser\tMERGEABLE\tmain\th%s\t%s\t%s\t%s\t%s\n' "$1" "$1" "$2" "$3" "${4:-}" "${5:-}"
  }
  prs=$(
    line 1 local/macos NONE
    line 1 local/ios NONE
    line 2 local/macos PENDING 2026-01-01T00:00:00Z Queued
    line 3 local/macos PENDING 2026-01-01T00:00:00Z "Queued; waiting for memory pressure to drop (now warning)"
    line 4 local/macos PENDING 2026-01-01T00:00:00Z "Hosted fallback: hosted CI on h4 (attempt 1); x"
    line 5 local/macos PENDING 2026-01-01T00:00:00Z "Running just test on main@abc"
    line 5 local/ios NONE
    line 6 local/macos SUCCESS 2026-01-01T00:00:00Z Passed
    line 6 local/ios SUCCESS 2026-01-01T00:00:00Z "Not affected: no core or ios change (main@abc)"
  )
  queue_new
  check "new and outdated queued rows get the waiting text, the running row does not" \
    [ "$(cut -f1,2 "$posts" | tr '\t\n' ': ')" = "h1:local/macos h1:local/ios h2:local/macos h5:local/ios " ]
  check "with the reason" grep -q "^h1	local/macos	Queued; waiting for memory pressure" "$posts"
  check "the queue keeps the posted text" \
    [ "$(awk -F'\t' '$1 == 2 { print $11 }' <<<"$prs")" = "$(queued_text)" ]
  # Oldest first; at the same time, a PR's desktop row before its mobile one.
  prs=$(
    line 2 local/ios PENDING 2026-01-01T00:00:05Z Queued
    line 2 local/macos PENDING 2026-01-01T00:00:05Z Queued
    line 1 local/ios PENDING 2026-01-01T00:00:07Z Queued
    line 3 local/macos PENDING 2026-01-01T00:00:01Z Queued
  )
  check "the queue runs oldest first, desktop before mobile" \
    [ "$(queue | cut -f1,8 | tr '\t\n' ': ')" = "3:local/macos 2:local/macos 2:local/ios 1:local/ios " ]
  # After a restart mid-job, the desktop row's status is newer than its
  # queued mobile row's: the desktop job still goes first.
  prs=$(
    line 7 local/macos PENDING 2026-01-01T00:00:09Z "Running just test on main@abc"
    line 7 local/ios PENDING 2026-01-01T00:00:02Z Queued
    line 8 local/macos PENDING 2026-01-01T00:00:03Z Queued
  )
  check "a PR's desktop job goes before its older queued mobile one" \
    [ "$(queue | cut -f1,8 | tr '\t\n' ': ')" = "7:local/macos 7:local/ios 8:local/macos " ]
  # A runner that comes back tests rows the fallback holds or finished.
  prs=$(
    line 1 local/macos SUCCESS 2026-01-01T00:00:01Z "Hosted fallback: clippy and test (macos-26) passed; just mobile not run"
    line 1 local/ios SUCCESS 2026-01-01T00:00:01Z "Hosted fallback: NOT tested; no hosted iOS job yet (ADR 0091 item 5); x"
    line 2 local/macos ERROR 2026-01-01T00:00:02Z "Hosted fallback: no hosted result after 3 dispatches; dispatch ci.yml by hand"
    line 3 local/macos PENDING 2026-01-01T00:00:03Z "Hosted fallback: hosted CI on h3 (attempt 1); x"
    line 4 local/macos SUCCESS 2026-01-01T00:00:04Z "Passed on main@abc in 3m00s"
    line 6 local/macos FAILURE 2026-01-01T00:00:06Z "Failed in just test on main@abc"
  )
  check "fallback rows, finished or not, are queued; local results are not" \
    [ "$(queue | cut -f1,8 | tr '\t\n' ': ')" = "1:local/macos 1:local/ios 2:local/macos 3:local/macos " ]
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
# Statuses: sha, context, state, description, target url, time.
cat >"$tmp/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -eu
latest() { # head context: "STATE<TAB>time<TAB>description"
  awk -F'\t' -v h="$1" -v c="$2" -v OFS='\t' '$1 == h && $2 == c { s = toupper($3); t = $6; d = $4 }
    END { print (s == "" ? "NONE" : s), t, d }' "$FAKE/statuses.tsv"
}
if [ "$1 $2" = "api graphql" ]; then
  while IFS=$'\t' read -r n draft cross author mergeable base head; do
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$n" "$draft" "$cross" "$author" \
      "$mergeable" "$base" "$head" "$(latest "$head" "$FAKE_CONTEXT")" "$(latest "$head" "$FAKE_MOBILE_CONTEXT")"
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
  [ "$context" = "$FAKE_CONTEXT" ] || [ "$context" = "$FAKE_MOBILE_CONTEXT" ] || exit 7
  seq=$(($(wc -l <"$FAKE/statuses.tsv") + 1))
  printf '%s\t%s\t%s\t%s\t%s\t2026-01-01T%02d:%02d:%02dZ\n' "$sha" "$context" "$state" "$desc" "$url" \
    $((seq / 3600)) $((seq / 60 % 60)) $((seq % 60)) >>"$FAKE/statuses.tsv"
  exit 0
fi
echo "fake gh: unexpected $*" >&2
exit 9
EOF

# Each PR's or main commit's behavior is its ci-behavior file: in the app or
# core folder it changes, so the file stays in that area, else at the root.
cat >"$tmp/bin/just" <<'EOF'
#!/usr/bin/env bash
set -eu
behavior=$({ cat apps/ios/ci-behavior apps/android/ci-behavior crates/ci-behavior ci-behavior 2>/dev/null || true; } | head -n 1)
echo "${behavior:-none} $* renders=${MDOC_HEADLESS_ARTIFACTS:+set} job=${MDOC_IOS_JOB:-}${MDOC_ANDROID_JOB:-}" >>"$FAKE/just.log"
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
  "arrive lint")
    # A PR head arrives while a main commit's job runs; the job goes on.
    cat "$FAKE/late-row" >>"$FAKE/prs.tsv"
    sleep 4
    ;;
esac
[ "$1" != test ] || touch "$MDOC_HEADLESS_ARTIFACTS"
EOF
chmod +x "$tmp/bin/gh" "$tmp/bin/just"

# --- origin with PR merge refs --------------------------------------------------

git_() { git -C "$tmp/work" -c user.name=ci -c user.email=ci@example.com "$@"; }
git init --quiet --bare -b main "$tmp/origin.git"
git init --quiet -b main "$tmp/work"
mkdir -p "$tmp/work/.github/workflows" "$tmp/work/src" "$tmp/work/docs" "$tmp/work/scripts" \
  "$tmp/work/apps/ios" "$tmp/work/apps/android" "$tmp/work/crates/mdoc-ffi"
cp .github/workflows/ci.yml "$tmp/work/.github/workflows/ci.yml"
tr -d '\r' <scripts/ci-areas.txt >"$tmp/work/scripts/ci-areas.txt"
echo base >"$tmp/work/src/lib.rs"
echo base >"$tmp/work/apps/ios/App.swift"
echo base >"$tmp/work/apps/android/App.kt"
echo base >"$tmp/work/crates/mdoc-ffi/lib.rs"
echo readme >"$tmp/work/README.md"
echo doc >"$tmp/work/docs/a.md"
: >"$tmp/work/scripts/mobile-check.sh"
: >"$tmp/work/scripts/android-check.sh"
: >"$tmp/work/scripts/detection-gate.sh"
cat >"$tmp/work/scripts/setup.sh" <<'EOF'
#!/usr/bin/env bash
# Setup must never wait on a prompt: the runner closes its stdin.
! read -r -t 1 _ || exit 5
behavior=$({ cat apps/ios/ci-behavior apps/android/ci-behavior crates/ci-behavior ci-behavior 2>/dev/null || true; } | head -n 1)
echo "${behavior:-none} setup" >>"$FAKE/just.log"
EOF
git_ add -A
git_ commit --quiet -m base
git_ push --quiet "$tmp/origin.git" main
base=$(git_ rev-parse HEAD)

# Commits a PR branch from main and its merge ref; prints the head. The
# behavior goes in $at (default the root, a desktop file).
pr() { # number behavior [file...]
  local n=$1 behavior=$2 f at=${at:-ci-behavior}
  shift 2
  git_ checkout --quiet -B "pr-$n" "$base"
  [ "$behavior" = docs ] || echo "$behavior" >"$tmp/work/$at"
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
docs=$(pr 2 docs README.md docs/b.md apps/ios/README.md)
conflict=$(pr 3 pass src/lib.rs)
draft=$(pr 4 pass src/lib.rs)
fork=$(pr 5 pass src/lib.rs)
bot=$(pr 6 pass src/lib.rs)
fail=$(pr 7 fail src/lib.rs)
slow=$(pr 8 slow src/lib.rs)
old=$(pr 9 push src/lib.rs)
done_before=$(pr 10 pass src/lib.rs)
stale=$(pr 11 pass src/lib.rs)
swift=$(at=apps/ios/ci-behavior pr 12 swift apps/ios/App.swift)
kotlin=$(at=apps/android/ci-behavior pr 13 kotlin apps/android/App.kt)
core=$(at=crates/ci-behavior pr 14 core crates/mdoc-ffi/lib.rs)
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
  row 12 "$swift"
  row 13 "$kotlin"
  row 14 "$core"
} >"$FAKE/prs.tsv"

# --- run ------------------------------------------------------------------------

# shellcheck disable=SC2031 # only the first subshell's config() sets it
export LOCAL_CI_HOME="$tmp/ci"
mkdir -p "$LOCAL_CI_HOME"
git clone --quiet "$tmp/origin.git" "$LOCAL_CI_HOME/mdoc"
case "$(platform)" in
  macos-arm64) FAKE_CONTEXT=local/macos mobile=ios ;;
  windows-x64) FAKE_CONTEXT=local/windows mobile=android ;;
  *)
    export LOCAL_CI_OS=test
    FAKE_CONTEXT=local/test mobile=ios
    ;;
esac
FAKE_MOBILE_CONTEXT=local/$mobile
export FAKE_CONTEXT FAKE_MOBILE_CONTEXT
D=$FAKE_CONTEXT M=$FAKE_MOBILE_CONTEXT
printf '%s\t%s\tsuccess\tPassed earlier\t\t2026-01-01T00:00:00Z\n' "$done_before" "$D" >>"$FAKE/statuses.tsv"
printf '%s\t%s\tpending\tRunning on main@0000000\t\t2026-01-01T00:00:00Z\n' "$stale" "$D" >>"$FAKE/statuses.tsv"
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
# main was tested up to its tip: this pass is about PRs.
echo "$base" >"$LOCAL_CI_HOME/state/main-last"

# The build cache survives a job's clean; other untracked files do not.
mkdir -p "$LOCAL_CI_HOME/mdoc/target/debug"
echo cache >"$LOCAL_CI_HOME/mdoc/target/debug/kept"
echo stray >"$LOCAL_CI_HOME/mdoc/stray.txt"

run_once() { # log
  bash scripts/local-ci.sh once >"$1" 2>&1 &
  local runner=$! rc=0
  for _ in $(seq 1 180); do
    kill -0 "$runner" 2>/dev/null || break
    sleep 1
  done
  if kill -0 "$runner" 2>/dev/null; then
    kill "$runner"
    echo "FAIL: the runner did not finish within 3 minutes" >&2
    failures=$((failures + 1))
  fi
  wait "$runner" || rc=$?
  check "the runner exits once nothing is queued" [ "$rc" -eq 0 ]
}
run_once "$tmp/runner.log"
kill "$mover" 2>/dev/null || true
wait "$mover" 2>/dev/null || true
check "a reused pid in the lock or job.pid is left alone" kill -0 "$unrelated"
kill "$unrelated" 2>/dev/null || true
wait "$unrelated" 2>/dev/null || true

# The last status posted on a head for a context: "state<TAB>description<TAB>url".
last() { awk -F'\t' -v h="$1" -v c="$2" -v OFS='\t' '$1 == h && $2 == c { s = $3; d = $4; u = $5 } END { print s, d, u }' "$FAKE/statuses.tsv"; }
# shellcheck disable=SC2053 # $3 is a glob
has() { [[ $(last "$1" "$2") == $3 ]]; }
has_both() { has "$1" "$D" "$2" && has "$1" "$M" "$2"; }
# True when the context's statuses on the head include a pending one.
was_pending() { awk -F'\t' -v h="$1" -v c="$2" '$1 == h && $2 == c && $3 == "pending" { f = 1 } END { exit !f }' "$FAKE/statuses.tsv"; }
short_base=${base:0:7}
ran() { grep "^$1 " "$FAKE/just.log" | cut -d' ' -f2- | sed 's/ renders=.*//' | tr '\n' ','; }

check "a passing PR gets success naming the base" has "$pass" "$D" "success	Passed on main@$short_base in *	https://ci.example/ci/pr-1/${pass:0:7}/"
check "it was queued, then ran" grep -q "^$pass	$D	pending	Running on main@$short_base	https://ci.example/ci/pr-1/" "$FAKE/statuses.tsv"
check "a step the runner sees running posts its own status" grep -q "^$slow	$D	pending	Running just test on main@$short_base	https://ci.example/ci/pr-8/" "$FAKE/statuses.tsv"
check "queued statuses link the runner's status" grep -q "^$pass	$D	pending	Queued	https://ci.example/ci/status.txt" "$FAKE/statuses.tsv"
steps_run() { grep "^$1 " "$FAKE/just.log" | head -3 | cut -d' ' -f2 | tr '\n' ' '; }
check "setup, lint, then test" [ "$(steps_run pass)" = "setup lint test " ]
case "$FAKE_CONTEXT" in
  local/macos) last_step="detection-gate" ;;
  *) last_step="test" ;;
esac
check "the Mac's desktop job ends with the detection gate, others with test" \
  [ "$(grep "^pass " "$FAKE/just.log" | tail -1 | cut -d' ' -f2- | sed 's/ renders=.*//')" = "$last_step" ]
check "lint gets no renders" grep -qx "pass lint renders= job=" "$FAKE/just.log"
check "the test step gets MDOC_HEADLESS_ARTIFACTS" grep -qx "pass test renders=set job=" "$FAKE/just.log"
check "the job's log and summary are kept" [ -s "$LOCAL_CI_HOME/out/pr-1/${pass:0:7}/log.txt" ]
check "the summary records the result" grep -q "^result: success" "$LOCAL_CI_HOME/out/pr-1/${pass:0:7}/summary.txt"
check "a desktop-only PR leaves $M not affected" has "$pass" "$M" "success	Not affected: no core or $mobile change (main@$short_base)	"
check "without queueing it" not was_pending "$pass" "$M"
check "a docs-only PR passes both statuses without a build" \
  has_both "$docs" "success	Docs only, not tested (main@$short_base)	"
check "docs-only never ran just" eval "! grep -q '^none' \"\$FAKE/just.log\""
check "a conflicting PR fails both statuses" has_both "$conflict" "failure	Conflicts with main	"
for skipped in "$draft" "$fork" "$bot"; do
  check "drafts, forks and bots get no status (${skipped:0:7})" eval "! grep -q '^$skipped' \"\$FAKE/statuses.tsv\""
done
check "a failing test fails, naming the step" has "$fail" "$D" "failure	Failed in just test on main@$short_base	*"
check "a test over its limit times out" has "$slow" "$D" "failure	Timed out in just test after 0m15s on main@$short_base	*"
check "a newer head supersedes a running job" has "$old" "$D" "error	Superseded by ${new:0:7}	*"
check "the newer head is tested" has "$new" "$D" "success	Passed on main@$short_base in *"
check "a tested head is not re-run" has "$done_before" "$D" "success	Passed earlier	"
check "a pending head left by a stopped runner re-runs" has "$stale" "$D" "success	Passed on main@$short_base in *"

# Areas: a Swift-only PR runs only the iOS job, an Android-only one only the
# Android job, a core one both statuses.
check "a Swift-only PR leaves $D not affected" has "$swift" "$D" "success	Not affected: no core or desktop change (main@$short_base)	"
check "at once, without queueing it" not was_pending "$swift" "$D"
check "a Kotlin-only PR leaves $D not affected" has "$kotlin" "$D" "success	Not affected: no core or desktop change (main@$short_base)	"
if [ "$mobile" = ios ]; then
  check "a Swift-only PR runs only the iOS job" [ "$(ran swift)" = "setup,mobile --ios," ]
  check "which the runner asks for" grep -qx "swift mobile --ios renders= job=always" "$FAKE/just.log"
  check "and posts as $M" has "$swift" "$M" "success	Passed on main@$short_base in *	https://ci.example/ci/pr-12/${swift:0:7}-ios/"
  check "into its own folder" [ -s "$LOCAL_CI_HOME/out/pr-12/${swift:0:7}-ios/log.txt" ]
  check "a Kotlin-only PR runs nothing here" [ -z "$(ran kotlin)" ]
  check "and leaves $M not affected" has "$kotlin" "$M" "success	Not affected: no core or ios change (main@$short_base)	"
  mobile_steps="setup,mobile --ios,"
else
  check "a Kotlin-only PR runs only the Android job" [ "$(ran kotlin)" = "setup,android," ]
  check "which the runner asks for" grep -qx "kotlin android renders= job=always" "$FAKE/just.log"
  check "and posts as $M" has "$kotlin" "$M" "success	Passed on main@$short_base in *	https://ci.example/ci/pr-13/${kotlin:0:7}-android/"
  check "a Swift-only PR runs nothing here" [ -z "$(ran swift)" ]
  check "and leaves $M not affected" has "$swift" "$M" "success	Not affected: no core or android change (main@$short_base)	"
  mobile_steps="setup,android,"
fi
check "a core PR runs both statuses" has_both "$core" "success	Passed on main@$short_base in *"
desktop_steps="setup,lint,test,"
[ "$D" != local/macos ] || desktop_steps+="mobile --ffi,detection-gate,"
check "the desktop job first, then the mobile one, which does not wait for it to post" \
  [ "$(ran core)" = "$desktop_steps$mobile_steps" ]
check "renders are kept for a PR's newest head only" [ ! -e "$LOCAL_CI_HOME/out/pr-1/0000000/renders" ]
check "an older head's log is kept" [ -e "$LOCAL_CI_HOME/out/pr-1/0000000/log.txt" ]
check "the status file is written" grep -q "^running: nothing" "$LOCAL_CI_HOME/out/status.txt"
check "the lock is released" [ ! -e "$LOCAL_CI_HOME/state/lock" ]
check "a job keeps target/" [ -e "$LOCAL_CI_HOME/mdoc/target/debug/kept" ]
check "a job removes other untracked files" [ ! -e "$LOCAL_CI_HOME/mdoc/stray.txt" ]

# --- main -----------------------------------------------------------------------

# Three commits land on main: a desktop one, one in this runner's mobile
# area and a docs one. A PR head arrives while the first one's job runs: it
# goes first, after that job.
git_ checkout --quiet --detach "$base"
echo arrive >"$tmp/work/ci-behavior"
echo m1 >>"$tmp/work/src/lib.rs"
git_ add -A
git_ commit --quiet -m "main 1"
m1=$(git_ rev-parse HEAD)
echo mob >"$tmp/work/apps/$mobile/ci-behavior"
echo m2 >>"$tmp/work/apps/$mobile/App.$([ "$mobile" = ios ] && echo swift || echo kt)"
git_ add -A
git_ commit --quiet -m "main 2"
m2=$(git_ rev-parse HEAD)
echo m3 >>"$tmp/work/docs/a.md"
git_ commit --quiet -am "main 3"
m3=$(git_ rev-parse HEAD)
git_ push --quiet "$tmp/origin.git" HEAD:main
late=$(pr 20 late src/lib.rs)
row 20 "$late" >"$FAKE/late-row"
: >"$FAKE/just.log"
run_once "$tmp/runner-main.log"
first() { grep -n "^$1 " "$FAKE/just.log" | head -1 | cut -d: -f1; }
final() { grep -n "^$1 " "$FAKE/just.log" | tail -1 | cut -d: -f1; }

check "a main commit's job posts on the commit" has "$m1" "$D" "success	Passed on main@${m1:0:7} in *	https://ci.example/ci/main/${m1:0:7}/"
check "in the areas that commit touched" has "$m1" "$M" "success	Not affected: no core or $mobile change (main@${m1:0:7})	"
check "a PR head arriving does not stop a main job" not grep -q "^$m1	$D	error" "$FAKE/statuses.tsv"
check "the PR is tested" has "$late" "$D" "success	Passed on main@$short_base in *"
check "after the main job" [ "$(first late)" -gt "$(final arrive)" ]
check "and before the next main commit" [ "$(first mob)" -gt "$(final late)" ]
check "the next main commit runs only its mobile job" [ "$(ran mob)" = "$mobile_steps" ]
check "and posts it" has "$m2" "$M" "success	Passed on main@${m2:0:7} in *"
check "beside a desktop not affected" has "$m2" "$D" "success	Not affected: no core or desktop change (main@${m2:0:7})	"
check "a docs-only main commit passes both without a build" \
  has_both "$m3" "success	Docs only, not tested (main@${m3:0:7})	"
check "main is tested up to its tip" [ "$(cat "$LOCAL_CI_HOME/state/main-last")" = "$m3" ]
check "a main without the runner scripts does not mark the copy outdated" not grep -q "^outdated" "$LOCAL_CI_HOME/out/status.txt"
check "with nothing left queued" [ ! -s "$LOCAL_CI_HOME/state/main-queue" ]
check "the status file shows the main queue" grep -q "^main queue: empty" "$LOCAL_CI_HOME/out/status.txt"

if [ "$failures" -gt 0 ]; then
  echo "--- runner log" >&2
  cat "$tmp/runner.log" "$tmp/runner-main.log" >&2 2>/dev/null || true
  echo "--- statuses" >&2
  cat "$FAKE/statuses.tsv" >&2
  echo "--- just" >&2
  cat "$FAKE/just.log" >&2
  die "$failures local CI check(s) failed"
fi
echo "local CI runner: all checks passed"
