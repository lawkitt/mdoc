#!/usr/bin/env bash
# Usage: scripts/test-ios-ui-shards.sh
# scripts/ios-ui-shards.sh (ADR 0112, ADR 0113): on the real tree every UI
# test class is in exactly one shard, and the shards print disjoint
# `-only-testing` lists that cover them all; on scratch copies the greedy
# longest-first packing by apps/ios/ui-class-times.tsv, a class the table
# lacks at the table's median, a gone class's row, and the errors (a class
# declared twice, a table row twice, a bad row, no table, a bad shard
# number); what counts as a test class (XCTestCase, also with the
# superclass on the next line or after generics; a name ending in Tests;
# not a helper class, `class func`, `class var` or a comment). Then
# scripts/ios-ui-class-times.py's table (and, off Windows, collect against
# a fake xcrun, a retried case counting its passing run) and
# scripts/ios-ui-class-times.sh's refresh against a local origin: no push
# when nothing moves or the branch already holds it, else one commit on main
# pushed to the branch, never to main. No network. Bash 3.2 compatible.
source "$(dirname "$0")/lib.sh"
root=$(pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# expect <rc> <script> [args...]: its output is in $tmp/out.
expect() {
  local want=$1 rc=0
  shift
  bash "$@" >"$tmp/out" 2>&1 || rc=$?
  [ "$rc" = "$want" ] || fail "$*: exited $rc, not $want: $(cat "$tmp/out")"
}

# The real tree.
expect 0 scripts/ios-ui-shards.sh --check
[ ! -s "$tmp/out" ] || fail "--check printed: $(cat "$tmp/out")"
expect 0 scripts/ios-ui-shards.sh 1
one=$(cat "$tmp/out")
expect 0 scripts/ios-ui-shards.sh 2
two=$(cat "$tmp/out")
if [ -z "$one" ] || [ -z "$two" ]; then fail "a shard is empty"; fi
# shellcheck disable=SC2086 # one identifier per word
both=$(printf '%s\n' $one $two | LC_ALL=C sort)
[ -z "$(printf '%s\n' "$both" | uniq -d)" ] || fail "in both shards: $(printf '%s\n' "$both" | uniq -d)"
for id in $one $two; do
  case "$id" in mdocUITests/*UITests) ;; *) fail "not an identifier: $id" ;; esac
done
want=$(grep -rhoE 'class +[A-Za-z0-9_]+ *: *XCTestCase' apps/ios/mdocUITests |
  awk '{ sub(/^class +/, ""); sub(/ *:.*/, ""); print "mdocUITests/" $0 }' | LC_ALL=C sort)
[ "$both" = "$want" ] || fail "the shards are not every XCTestCase class: $(diff <(echo "$both") <(echo "$want") || true)"
# The plan covers every class, and its totals are the two shards'.
expect 0 scripts/ios-ui-shards.sh --plan
[ "$(grep -c '^[12] ' "$tmp/out")" = "$(printf '%s
' "$want" | wc -l | tr -d ' ')" ] || fail "plan: $(cat "$tmp/out")"
grep -q '^shard 2: ' "$tmp/out" || fail "plan totals: $(cat "$tmp/out")"
expect 1 scripts/ios-ui-shards.sh 3
grep -q 'no shard 3' "$tmp/out" || fail "shard 3: $(cat "$tmp/out")"
expect 1 scripts/ios-ui-shards.sh
expect 1 scripts/ios-ui-shards.sh 1 2

# Scratch classes and tables: MDOC_UI_TESTS_DIR and MDOC_UI_CLASS_TIMES.
s=scripts/ios-ui-shards.sh
export MDOC_UI_TESTS_DIR="$tmp/ui" MDOC_UI_CLASS_TIMES="$tmp/times.tsv"
# classes <name>...: a fresh tests folder with these classes.
classes() {
  rm -rf "$tmp/ui"
  mkdir -p "$tmp/ui"
  local c
  for c in "$@"; do
    printf 'import XCTest\n\nfinal class %s: XCTestCase {\n    func testIt() {}\n}\n' "$c" >"$tmp/ui/$c.swift"
  done
}
# times <class seconds>...: the table, with a comment and a parts column.
times() {
  {
    echo "# scratch"
    while [ $# -gt 0 ]; do
      printf '%s\t%s\t3\n' "$1" "$2"
      shift 2
    done
  } >"$tmp/times.tsv"
}
# shard <n> <class>...: shard n is exactly these, in this order.
shard() {
  local n=$1
  shift
  expect 0 "$s" "$n"
  local w
  w=$(printf 'mdocUITests/%s ' "$@")
  [ "$(cat "$tmp/out")" = "${w% }" ] || fail "shard $n: $(cat "$tmp/out"), not ${w% }"
}

# Longest first, each to the shard with less so far; on a tie of time, the
# one with fewer classes, then the lower number.
classes AUITests BUITests CUITests DUITests EUITests
times AUITests 100 BUITests 90 CUITests 60 DUITests 50 EUITests 40
shard 1 AUITests DUITests EUITests
shard 2 BUITests CUITests
expect 0 "$s" --plan
grep -qx 'shard 1: 3 classes, 190 s' "$tmp/out" || fail "plan: $(cat "$tmp/out")"
grep -qx 'shard 2: 2 classes, 150 s' "$tmp/out" || fail "plan: $(cat "$tmp/out")"
grep -qx '1 AUITests 100' "$tmp/out" || fail "plan: $(cat "$tmp/out")"
# A tie on time goes by name; on a tie of totals, to fewer classes.
classes AUITests BUITests CUITests DUITests
times AUITests 60 CUITests 30 BUITests 30 DUITests 0
shard 1 AUITests DUITests
shard 2 BUITests CUITests
classes AUITests BUITests CUITests DUITests EUITests
# Not by the order of the table or the files; a fraction counts (150.4 is
# more than 150).
times EUITests 40 DUITests 50 CUITests 60 BUITests 90 AUITests 100.4
shard 1 AUITests DUITests
shard 2 BUITests CUITests EUITests

# A class the table lacks counts as its median (60 of 100, 90, 60, 50, 40),
# placed by name among equal times, and the plan marks it.
classes AUITests BUITests CUITests DUITests EUITests FUITests
times AUITests 100 BUITests 90 CUITests 60 DUITests 50 EUITests 40
shard 1 AUITests FUITests EUITests
shard 2 BUITests CUITests DUITests
expect 0 "$s" --plan
grep -qx '1 FUITests 60 median' "$tmp/out" || fail "median: $(cat "$tmp/out")"
# An even count: the mean of the middle two (75 of 100, 90, 60, 50); a
# gone class's row counts for the median only.
classes AUITests BUITests CUITests NewUITests
times AUITests 100 BUITests 90 CUITests 60 GoneUITests 50
expect 0 "$s" --plan
grep -qx '2 NewUITests 75 median' "$tmp/out" || fail "even median: $(cat "$tmp/out")"
! grep -q GoneUITests "$tmp/out" || fail "a gone class planned: $(cat "$tmp/out")"
# An empty table: every class at 0, spread by count.
classes AUITests BUITests CUITests
times
shard 1 AUITests CUITests
shard 2 BUITests
expect 0 "$s" --classes
[ "$(cat "$tmp/out")" = "$(printf 'AUITests\nBUITests\nCUITests')" ] || fail "--classes: $(cat "$tmp/out")"

# The errors, each by name.
classes AUITests BUITests
times AUITests 10 BUITests 20 AUITests 30
expect 1 "$s" --check
grep -q 'AUITests is in .* more than once' "$tmp/out" || fail "a row twice: $(cat "$tmp/out")"
printf 'AUITests\t10\nBUITests\tslow\n' >"$tmp/times.tsv"
expect 1 "$s" 1
grep -q 'times.tsv:2 is not <class><tab><seconds>: BUITests' "$tmp/out" || fail "a bad row: $(cat "$tmp/out")"
printf 'AUITests 10\n' >"$tmp/times.tsv"
expect 1 "$s" --check
grep -q 'times.tsv:1 is not' "$tmp/out" || fail "spaces, not a tab: $(cat "$tmp/out")"
# CRLF lines read as LF ones.
printf 'AUITests\t10\r\nBUITests\t20\r\n' >"$tmp/times.tsv"
shard 1 BUITests
rm "$tmp/times.tsv"
expect 1 "$s" --check
grep -q "no $tmp/times.tsv" "$tmp/out" || fail "no table: $(cat "$tmp/out")"
times
printf 'final class AUITests: XCTestCase {}\n' >"$tmp/ui/Again.swift"
expect 1 "$s" --check
grep -q 'AUITests is declared more than once' "$tmp/out" || fail "declared twice: $(cat "$tmp/out")"

# What counts as a test class.
classes AUITests
cat >"$tmp/ui/Helpers.swift" <<'EOF'
import XCTest

final class LaunchHelper {
    class func make() -> LaunchHelper { LaunchHelper() }
}
// final class CommentedOutUITests: XCTestCase {}
class OpenUITests:XCTestCase {}
class SubclassedTests: OpenUITests {}
final class RegressionCase:
    XCTestCase {
    override class func setUp() {}
    class var shared: XCTestCase { fatalError() }
    func testRegression() {}
}
@MainActor final class GenericCase<T>: XCTest.XCTestCase where T: Equatable {}
EOF
expect 0 "$s" --classes
[ "$(cat "$tmp/out")" = "$(printf 'AUITests\nGenericCase\nOpenUITests\nRegressionCase\nSubclassedTests')" ] ||
  fail "test classes: $(cat "$tmp/out")"
unset MDOC_UI_TESTS_DIR MDOC_UI_CLASS_TIMES

# scripts/ios-ui-class-times.py.
py=$(command -v python3 || command -v python) || fail "no python"
mkdir -p "$tmp/parts"
printf 'AUITests\t100.4\nBUITests\t20\n' >"$tmp/parts/1.tsv"
printf 'AUITests\t90\nCUITests\t5.5\n' >"$tmp/parts/2.tsv"
printf 'AUITests\t300\n' >"$tmp/parts/3.tsv"
"$py" -I scripts/ios-ui-class-times.py table "$tmp/parts" --days 9 >"$tmp/out" 2>&1 || fail "table: $(cat "$tmp/out")"
grep -q '^# .*the last 9 days' "$tmp/out" || fail "table header: $(cat "$tmp/out")"
[ "$(grep -v '^#' "$tmp/out")" = "$(printf 'AUITests\t100\t3\nBUITests\t20\t1\nCUITests\t6\t1')" ] ||
  fail "table rows: $(cat "$tmp/out")"
printf 'AUITests\tslow\n' >"$tmp/parts/4.tsv"
! "$py" -I scripts/ios-ui-class-times.py table "$tmp/parts" >"$tmp/out" 2>&1 || fail "a bad part read"
grep -q '4.tsv:1' "$tmp/out" || fail "bad part: $(cat "$tmp/out")"
rm "$tmp/parts/4.tsv"

# collect, against a fake xcrun (Windows' Python runs no shell script).
if [ "$(platform)" != windows-x64 ]; then
  mkdir -p "$tmp/bin" "$tmp/xc/one.xcresult"
  cat >"$tmp/tests.json" <<'EOF'
{"testNodes": [{"nodeType": "Test Plan", "name": "mdoc", "children": [
  {"nodeType": "Unit test bundle", "name": "mdocTests", "children": [
    {"nodeType": "Test Suite", "name": "CoreTests", "children": [
      {"nodeType": "Test Case", "nodeIdentifier": "CoreTests/testA()", "durationInSeconds": 50}]}]},
  {"nodeType": "UI test bundle", "name": "mdocUITests", "children": [
    {"nodeType": "Test Suite", "name": "AUITests", "children": [
      {"nodeType": "Test Case", "nodeIdentifier": "AUITests/testA()", "durationInSeconds": 12.25, "result": "Passed"},
      {"nodeType": "Test Case", "nodeIdentifier": "AUITests/testB()", "duration": "1m 3s", "result": "Passed"}]},
    {"nodeType": "Test Suite", "name": "BUITests", "children": [
      {"nodeType": "Test Case", "nodeIdentifier": "BUITests/testSkip()", "durationInSeconds": 0.33, "result": "Skipped",
       "children": [{"nodeType": "Failure Message", "name": "Test skipped"}]},
      {"nodeType": "Test Case", "nodeIdentifier": "BUITests/testRetried()", "durationInSeconds": 47.5, "result": "Passed",
       "children": [{"nodeType": "Repetition", "name": "First Run", "durationInSeconds": 40, "result": "Failed"},
                    {"nodeType": "Repetition", "name": "Retry 1", "durationInSeconds": 7.5, "result": "Passed"}]}]}]}]}]}
EOF
  printf '#!/usr/bin/env bash\n[ "$*" = "xcresulttool get test-results tests --path %s" ] || exit 9\ncat %s\n' \
    "$tmp/xc/one.xcresult" "$tmp/tests.json" >"$tmp/bin/xcrun"
  chmod +x "$tmp/bin/xcrun"
  PATH="$tmp/bin:$PATH" "$py" -I scripts/ios-ui-class-times.py collect "$tmp/xc" >"$tmp/out" 2>&1 || fail "collect: $(cat "$tmp/out")"
  # A case passed on its retry counts its passing run: B is 0.33 + 7.5.
  [ "$(cat "$tmp/out")" = "$(printf 'AUITests\t75.2\nBUITests\t7.8')" ] || fail "collect: $(cat "$tmp/out")"
  ! "$py" -I scripts/ios-ui-class-times.py collect "$tmp/parts" >"$tmp/out" 2>&1 || fail "collect without an xcresult"
  grep -q 'no .xcresult' "$tmp/out" || fail "collect without an xcresult: $(cat "$tmp/out")"
fi

# scripts/ios-ui-class-times.sh: a scratch repository whose origin is a
# local bare one; with --from it reads nothing from GitHub, so gh fails.
repo=$tmp/repo
mkdir -p "$repo/scripts" "$repo/apps/ios" "$tmp/bin"
cp scripts/lib.sh scripts/ios-ui-shards.sh scripts/ios-ui-class-times.sh scripts/ios-ui-class-times.py "$repo/scripts/"
export MDOC_UI_TESTS_DIR="$tmp/ui"
classes AUITests BUITests CUITests DUITests
printf 'AUITests\t100\t1\nBUITests\t90\t1\nCUITests\t60\t1\n' >"$repo/apps/ios/ui-class-times.tsv"
git init -q --bare "$tmp/origin.git"
(
  cd "$repo"
  git init -q -b main
  git config core.autocrlf false
  git add -A
  git -c user.name=t -c user.email=t@example.com commit -q -m base
  git remote add origin "$tmp/origin.git"
  git push -q origin main
)
main=$(git -C "$repo" rev-parse main)
printf '#!/usr/bin/env bash\necho "gh called: $*" >&2\nexit 1\n' >"$tmp/bin/gh"
chmod +x "$tmp/bin/gh"
# refresh <rc> [args...]: runs the copy's refresh on the parts in $tmp/parts,
# from main, with its job summary in $tmp/summary.
refresh() {
  local want=$1 rc=0
  shift
  git -C "$repo" checkout -q main
  rm -f "$tmp/summary"
  PATH="$tmp/bin:$PATH" GITHUB_STEP_SUMMARY="$tmp/summary" \
    bash "$repo/scripts/ios-ui-class-times.sh" --from "$tmp/parts" "$@" >"$tmp/out" 2>&1 || rc=$?
  [ "$rc" = "$want" ] || fail "refresh $*: exited $rc, not $want: $(cat "$tmp/out")"
}
# pushed: the branch's commit on origin (empty: none).
pushed() { git -C "$tmp/origin.git" rev-parse -q --verify refs/heads/nightly/ui-class-times || true; }
rm -f "$tmp/parts"/*.tsv
# The same split, D still unmeasured: nothing to do, nothing pushed.
printf 'AUITests\t101\nBUITests\t88\nCUITests\t61\n' >"$tmp/parts/1.tsv"
refresh 0
grep -q 'nothing to do' "$tmp/out" || fail "unchanged: $(cat "$tmp/out")"
[ -z "$(git -C "$repo" status --porcelain)" ] || fail "unchanged wrote: $(git -C "$repo" status --porcelain)"
[ -z "$(pushed)" ] || fail "unchanged pushed"
[ ! -e "$tmp/summary" ] || fail "unchanged summary: $(cat "$tmp/summary")"
# D measured, and a gone class's times: --dry-run writes the table only.
printf 'DUITests\t30\nGoneUITests\t500\n' >"$tmp/parts/2.tsv"
refresh 0 --dry-run
grep -qx '1 DUITests 30' "$tmp/out" || fail "dry run plan: $(cat "$tmp/out")"
grep -q '^DUITests	30	1$' "$repo/apps/ios/ui-class-times.tsv" || fail "dry run table: $(cat "$repo/apps/ios/ui-class-times.tsv")"
! grep -q GoneUITests "$repo/apps/ios/ui-class-times.tsv" || fail "a gone class kept"
[ -z "$(pushed)" ] || fail "dry run pushed"
git -C "$repo" checkout -q -- apps/ios/ui-class-times.tsv
# For real: one commit on main, force-pushed to the branch, and a notice and
# a job summary that name it; main never moves.
refresh 0
first=$(pushed)
[ -n "$first" ] || fail "not pushed: $(cat "$tmp/out")"
[ "$(git -C "$tmp/origin.git" rev-parse "$first~1")" = "$main" ] || fail "the branch is not one commit on main"
git -C "$tmp/origin.git" show "$first:apps/ios/ui-class-times.tsv" | grep -q '^DUITests	30	1$' || fail "pushed table"
git -C "$tmp/origin.git" log -1 --format=%B "$first" | grep -q 'Newly measured: DUITests' || fail "commit message"
grep -q '^::notice title=UI class times::A refresh of apps/ios/ui-class-times.tsv waits on the branch nightly/ui-class-times: open a PR from it. Moved: CUITests (1 → 2), DUITests (2 → 1). Newly measured: DUITests.$' "$tmp/out" ||
  fail "notice: $(cat "$tmp/out")"
grep -q 'waits on the branch nightly/ui-class-times' "$tmp/summary" || fail "summary: $(cat "$tmp/summary" 2>&1)"
[ "$(git -C "$tmp/origin.git" rev-parse main)" = "$main" ] || fail "main moved"
# The same refresh again: the branch already holds it, so no push.
refresh 0
grep -q 'already holds this refresh' "$tmp/out" || fail "again: $(cat "$tmp/out")"
[ "$(pushed)" = "$first" ] || fail "pushed the same refresh again"
# A row changes only with 3 parts: A at 10 in two more (3 in all) replaces
# its 100; B and C, measured once, keep theirs.
printf 'AUITests\t10\n' >"$tmp/parts/3.tsv"
printf 'AUITests\t10\n' >"$tmp/parts/4.tsv"
refresh 0 --dry-run
t=$repo/apps/ios/ui-class-times.tsv
grep -q '^AUITests	10	3$' "$t" || fail "3 parts: $(cat "$t")"
grep -q '^BUITests	90	1$' "$t" || fail "B kept: $(cat "$t")"
grep -q '^CUITests	60	1$' "$t" || fail "C kept: $(cat "$t")"
[ "$(grep -vc '^#' "$t")" = 4 ] || fail "not 4 rows: $(cat "$t")"
grep -q '^# .*3 parts or more' "$t" || fail "no header: $(cat "$t")"
git -C "$repo" checkout -q -- apps/ios/ui-class-times.tsv
# That moves B and C (A 100, B 90, C 60, D 90 as the median before; now A
# 10, B 90, C 60, D 30): a new push over the old one, naming them.
refresh 0
second=$(pushed)
if [ -z "$second" ] || [ "$second" = "$first" ]; then fail "not pushed again: $(cat "$tmp/out")"; fi
grep -q 'Moved: BUITests (2 → 1), CUITests (1 → 2)\.' "$tmp/out" || fail "moved: $(cat "$tmp/out")"
[ "$(git -C "$tmp/origin.git" rev-parse "$second~1")" = "$main" ] || fail "not on main"
[ "$(git -C "$tmp/origin.git" rev-parse main)" = "$main" ] || fail "main moved"
# Once that refresh is merged nothing is due, and the branch goes; with no
# branch, nothing is deleted.
git -C "$repo" checkout -q main
git -C "$repo" fetch -q origin nightly/ui-class-times
git -C "$repo" merge -q --ff-only FETCH_HEAD
git -C "$repo" push -q origin main
refresh 0
grep -q 'nothing to do' "$tmp/out" || fail "merged: $(cat "$tmp/out")"
grep -q '^::notice title=UI class times::Deleted nightly/ui-class-times' "$tmp/out" || fail "not deleted: $(cat "$tmp/out")"
[ -z "$(pushed)" ] || fail "the branch is still there"
refresh 0
! grep -q 'Deleted' "$tmp/out" || fail "deleted twice: $(cat "$tmp/out")"
refresh 1 --days x
cd "$root"

echo "ios-ui-shards: ok"
