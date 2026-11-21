#!/usr/bin/env bash
# Usage: scripts/ios/test-ios-ui-shards.sh
# scripts/ios/ios-ui-shards.sh: on the real tree every UI
# test class is in exactly one shard, and the shards print disjoint
# `-only-testing` lists that cover them all; on scratch copies the greedy
# longest-first packing by apps/ios/ui-class-times.tsv, a class the table
# lacks at the table's median, a gone class's row, and the errors (a class
# declared twice, a table row twice, a bad row, no table, a bad shard
# number); what counts as a test class (XCTestCase, also with the
# superclass on the next line or after generics; a name ending in Tests;
# not a helper class, `class func`, `class var` or a comment). Then
# scripts/ios/ios-ui-class-times.py's table, the artifacts fetch reads (the
# tests pin's only), and, off Windows, collect against a fake xcrun (a
# retried case counting its passing run); and
# scripts/ios/ios-ui-class-times.sh's refresh against a local origin: no push
# when nothing moves or the branch already holds it, else one commit on main
# pushed to the branch, never to main. No network. Bash 3.2 compatible.
source "$(dirname "$0")/../lib.sh"
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
expect 0 scripts/ios/ios-ui-shards.sh --check
[ ! -s "$tmp/out" ] || fail "--check printed: $(cat "$tmp/out")"
expect 0 scripts/ios/ios-ui-shards.sh 1
one=$(cat "$tmp/out")
expect 0 scripts/ios/ios-ui-shards.sh 2
two=$(cat "$tmp/out")
expect 0 scripts/ios/ios-ui-shards.sh 3
three=$(cat "$tmp/out")
if [ -z "$one" ] || [ -z "$two" ] || [ -z "$three" ]; then fail "a shard is empty"; fi
# shellcheck disable=SC2086 # one identifier per word
both=$(printf '%s\n' $one $two $three | LC_ALL=C sort)
[ -z "$(printf '%s\n' "$both" | uniq -d)" ] || fail "in two shards: $(printf '%s\n' "$both" | uniq -d)"
for id in $one $two $three; do
  case "$id" in mdocUITests/*UITests) ;; *) fail "not an identifier: $id" ;; esac
done
want=$(grep -rhoE 'class +[A-Za-z0-9_]+ *: *XCTestCase' apps/ios/mdocUITests |
  awk '{ sub(/^class +/, ""); sub(/ *:.*/, ""); print "mdocUITests/" $0 }' | LC_ALL=C sort)
[ "$both" = "$want" ] || fail "the shards are not every XCTestCase class: $(diff <(echo "$both") <(echo "$want") || true)"
# The plan covers every class, and its totals are the three shards'.
expect 0 scripts/ios/ios-ui-shards.sh --plan
[ "$(grep -c '^[123] ' "$tmp/out")" = "$(printf '%s
' "$want" | wc -l | tr -d ' ')" ] || fail "plan: $(cat "$tmp/out")"
grep -q '^shard 3: ' "$tmp/out" || fail "plan totals: $(cat "$tmp/out")"
expect 1 scripts/ios/ios-ui-shards.sh 4
grep -q 'no shard 4' "$tmp/out" || fail "shard 4: $(cat "$tmp/out")"
expect 1 scripts/ios/ios-ui-shards.sh
expect 1 scripts/ios/ios-ui-shards.sh 1 2

# Scratch classes and tables: MDOC_UI_TESTS_DIR and MDOC_UI_CLASS_TIMES,
# packed into two shards (MDOC_UI_SHARDS) but where a case says three.
s=scripts/ios/ios-ui-shards.sh
export MDOC_UI_TESTS_DIR="$tmp/ui" MDOC_UI_CLASS_TIMES="$tmp/times.tsv" MDOC_UI_SHARDS=2
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
# Three shards, as ci-ios.yml runs: the third takes C, then the least
# loaded takes each of D and E.
MDOC_UI_SHARDS=3
times AUITests 100 BUITests 90 CUITests 60 DUITests 50 EUITests 40
shard 1 AUITests
shard 2 BUITests EUITests
shard 3 CUITests DUITests
MDOC_UI_SHARDS=2

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

# --touched: the shards holding the classes the files declare (shard 1: A,
# D, E; shard 2: B, C); a file there declaring none (a helper) touches
# every shard; a deleted file, a file elsewhere or none touch none.
classes AUITests BUITests CUITests DUITests EUITests
times AUITests 100 BUITests 90 CUITests 60 DUITests 50 EUITests 40
touched() { # <shards> <file>...
  local want=$1 got
  shift
  expect 0 "$s" --touched "$@"
  got=$(cat "$tmp/out")
  [ "${got//$'\n'/ }" = "$want" ] || fail "--touched $*: ${got//$'\n'/ }, not $want"
}
touched 2 "$tmp/ui/BUITests.swift"
touched "1 2" "$tmp/ui/DUITests.swift" "$tmp/ui/CUITests.swift" "$tmp/ui/EUITests.swift" README.md
printf 'import XCTest\n\nfunc waitLong() {}\n' >"$tmp/ui/Waits.swift"
touched "1 2" "$tmp/ui/Waits.swift"
rm "$tmp/ui/Waits.swift"
touched "" "$tmp/ui/GoneUITests.swift" "$tmp/ui/notes.md" "$tmp/BUITests.swift"
touched ""

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

# scripts/ios/ios-ci-parts.sh in a scratch repository, its main as the
# branch's main (MDOC_MAIN_REF), two shards (shard 1: A; shard 2: B, C).
pr=$tmp/parts-repo
mkdir -p "$pr/scripts/ios" "$pr/apps/ios/mdocUITests" "$pr/apps/ios/mdocTests"
cp scripts/lib.sh "$pr/scripts/"
cp scripts/ios/ios-ui-shards.sh scripts/ios/ios-ci-parts.sh "$pr/scripts/ios/"
for c in AUITests BUITests CUITests; do
  printf 'import XCTest\n\nfinal class %s: XCTestCase {\n    func testIt() {}\n}\n' "$c" >"$pr/apps/ios/mdocUITests/$c.swift"
done
printf 'import XCTest\n\nfunc waitLong() {}\n' >"$pr/apps/ios/mdocUITests/Waits.swift"
printf 'final class CoreTests {}\n' >"$pr/apps/ios/mdocTests/CoreTests.swift"
printf 'AUITests\t100\nBUITests\t90\nCUITests\t60\n' >"$pr/apps/ios/ui-class-times.tsv"
git -C "$pr" init -q -b main
git -C "$pr" config core.autocrlf false
commit() { # <message>
  git -C "$pr" add -A
  git -C "$pr" -c user.name=t -c user.email=t@example.com commit -q -m "$1"
}
commit base
export MDOC_MAIN_REF=main
# parts <json> <spec>: the script prints that list.
parts() {
  expect 0 "$pr/scripts/ios/ios-ci-parts.sh" "$2"
  [ "$(cat "$tmp/out")" = "$1" ] || fail "parts $2: $(cat "$tmp/out"), not $1"
}
parts '["unit","ui 1","ui 2"]' all
parts '["unit","ui 2"]' 'ui2, unit'
parts '["ui 2"]' 'ui 2,ui2'
expect 1 "$pr/scripts/ios/ios-ci-parts.sh" 'ui 3'
grep -q 'no part ui 3: there are 2 UI shards' "$tmp/out" || fail "ui 3: $(cat "$tmp/out")"
expect 1 "$pr/scripts/ios/ios-ci-parts.sh" 'unit,lint'
grep -q "no part 'lint'" "$tmp/out" || fail "lint: $(cat "$tmp/out")"
expect 1 "$pr/scripts/ios/ios-ci-parts.sh" ''
# Nothing touched, on main itself: the unit part.
parts '["unit"]' touched
# A UI-only branch: one shard, by its merge base, not by main's tip, which
# moved on (A, in shard 1).
git -C "$pr" checkout -q -b ui-only
echo '// more' >>"$pr/apps/ios/mdocUITests/CUITests.swift"
commit ui
git -C "$pr" checkout -q main
echo '// later' >>"$pr/apps/ios/mdocUITests/AUITests.swift"
commit main
git -C "$pr" checkout -q ui-only
parts '["ui 2"]' touched
# A unit test changed too adds the unit part; a helper, every shard.
echo '// more' >>"$pr/apps/ios/mdocTests/CoreTests.swift"
commit unit
parts '["unit","ui 2"]' touched
echo '// more' >>"$pr/apps/ios/mdocUITests/Waits.swift"
commit helper
parts '["unit","ui 1","ui 2"]' touched
# No UI test class left (app code, a deleted class): the unit part.
git -C "$pr" checkout -q -b app main
git -C "$pr" rm -q apps/ios/mdocUITests/BUITests.swift
echo '// app' >"$pr/apps/ios/App.swift"
commit app
parts '["unit"]' touched
expect 1 "$pr/scripts/ios/ios-ci-parts.sh" touched x
MDOC_MAIN_REF=nowhere expect 1 "$pr/scripts/ios/ios-ci-parts.sh" touched
grep -q 'no merge base of HEAD and nowhere' "$tmp/out" || fail "no main: $(cat "$tmp/out")"
unset MDOC_MAIN_REF

# scripts/ios/ios-ui-class-times.py.
py=$(command -v python3 || command -v python) || fail "no python"
mkdir -p "$tmp/parts"
printf 'AUITests\t100.4\nBUITests\t20\n' >"$tmp/parts/1.tsv"
printf 'AUITests\t90\nCUITests\t5.5\n' >"$tmp/parts/2.tsv"
printf 'AUITests\t300\n' >"$tmp/parts/3.tsv"
"$py" -I scripts/ios/ios-ui-class-times.py table "$tmp/parts" --days 9 >"$tmp/out" 2>&1 || fail "table: $(cat "$tmp/out")"
grep -q '^# .*the last 9 days' "$tmp/out" || fail "table header: $(cat "$tmp/out")"
[ "$(grep -v '^#' "$tmp/out")" = "$(printf 'AUITests\t100\t3\nBUITests\t20\t1\nCUITests\t6\t1')" ] ||
  fail "table rows: $(cat "$tmp/out")"
printf 'AUITests\tslow\n' >"$tmp/parts/4.tsv"
! "$py" -I scripts/ios/ios-ui-class-times.py table "$tmp/parts" >"$tmp/out" 2>&1 || fail "a bad part read"
grep -q '4.tsv:1' "$tmp/out" || fail "bad part: $(cat "$tmp/out")"
rm "$tmp/parts/4.tsv"
# fetch reads only the tests pin's artifacts (macos-26), not the builds
# pin's or those named before the pin was (xcode-27's).
"$py" -I - scripts/ios/ios-ui-class-times.py >"$tmp/out" 2>&1 <<'EOF' || fail "artifact names: $(cat "$tmp/out")"
import importlib.util, sys
spec = importlib.util.spec_from_file_location("times", sys.argv[1])
times = importlib.util.module_from_spec(spec)
spec.loader.exec_module(times)
names = ["ios-ui-times-tests-ui1-1", "ios-ui-times-tests-ui3-2", "ios-ui-times-builds-ui1-1", "ios-ui-times-ui1-1", "ios-xcresult-ui1-1"]
print(" ".join(n for n in names if times.ARTIFACT.match(n)))
EOF
[ "$(cat "$tmp/out")" = "ios-ui-times-tests-ui1-1 ios-ui-times-tests-ui3-2" ] || fail "artifact names: $(cat "$tmp/out")"

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
  PATH="$tmp/bin:$PATH" "$py" -I scripts/ios/ios-ui-class-times.py collect "$tmp/xc" >"$tmp/out" 2>&1 || fail "collect: $(cat "$tmp/out")"
  # A case passed on its retry counts its passing run: B is 0.33 + 7.5.
  [ "$(cat "$tmp/out")" = "$(printf 'AUITests\t75.2\nBUITests\t7.8')" ] || fail "collect: $(cat "$tmp/out")"
  ! "$py" -I scripts/ios/ios-ui-class-times.py collect "$tmp/parts" >"$tmp/out" 2>&1 || fail "collect without an xcresult"
  grep -q 'no .xcresult' "$tmp/out" || fail "collect without an xcresult: $(cat "$tmp/out")"
fi

# scripts/ios/ios-ui-class-times.sh: a scratch repository whose origin is a
# local bare one; with --from it reads nothing from GitHub, so gh fails.
repo=$tmp/repo
mkdir -p "$repo/scripts/ios" "$repo/apps/ios" "$tmp/bin"
cp scripts/lib.sh "$repo/scripts/"
cp scripts/ios/ios-ui-shards.sh scripts/ios/ios-ui-class-times.sh scripts/ios/ios-ui-class-times.py "$repo/scripts/ios/"
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
    bash "$repo/scripts/ios/ios-ui-class-times.sh" --from "$tmp/parts" "$@" >"$tmp/out" 2>&1 || rc=$?
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
