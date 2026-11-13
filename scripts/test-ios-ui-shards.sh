#!/usr/bin/env bash
# Usage: scripts/test-ios-ui-shards.sh
# scripts/ios-ui-shards.sh (ADR 0112): on the real tree every UI test class
# is in exactly one shard, and the shards print disjoint `-only-testing`
# lists that cover them all; on scratch copies a new class in no shard, a
# class in two, a listed class that is gone and a bad shard number fail and
# name it; what counts as a test class (XCTestCase, also with the
# superclass on the next line or after generics; a name ending in Tests;
# not a helper class, `class func`, `class var` or a comment). Bash 3.2 compatible.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# expect <rc> <script> [args...]: runs it with the tests folder in $tmp/ui
# when MDOC_UI_TESTS_DIR is set; its output is in $tmp/out.
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
expect 1 scripts/ios-ui-shards.sh 3
grep -q 'no shard 3' "$tmp/out" || fail "shard 3: $(cat "$tmp/out")"
expect 1 scripts/ios-ui-shards.sh
expect 1 scripts/ios-ui-shards.sh 1 2

# A scratch copy, whose list the cases below edit; lib.sh moves to the root
# of the copy, and the copy reads the real classes from $tmp/ui.
mkdir -p "$tmp/repo/scripts"
cp scripts/lib.sh "$tmp/repo/scripts/"
cp -R apps/ios/mdocUITests "$tmp/ui"
export MDOC_UI_TESTS_DIR="$tmp/ui"
copy() {
  cp scripts/ios-ui-shards.sh "$tmp/repo/scripts/"
  rm -rf "$tmp/ui"
  cp -R apps/ios/mdocUITests "$tmp/ui"
}
s="$tmp/repo/scripts/ios-ui-shards.sh"
# edit <awk program>: rewrites the copy's script (BSD sed has no \n).
edit() {
  awk "$1" "$s" >"$s.new"
  mv "$s.new" "$s"
}

copy
expect 0 "$s" --check

# A new class in no shard fails, by name, and so does each shard.
printf 'import XCTest\n\n@MainActor\nfinal class BrandNewUITests: XCTestCase {\n    func testIt() {}\n}\n' >"$tmp/ui/BrandNewUITests.swift"
expect 1 "$s" --check
grep -q 'BrandNewUITests .*is in no shard' "$tmp/out" || fail "no shard: $(cat "$tmp/out")"
expect 1 "$s" 1
expect 1 "$s" 2
# Added to a shard, it passes and runs there.
# shellcheck disable=SC2016 # an awk program
edit '{ print } $0 == "2 OCRResumeUITests 0" { print "2 BrandNewUITests 30" }'
expect 0 "$s" 2
grep -q 'mdocUITests/BrandNewUITests' "$tmp/out" || fail "not in shard 2: $(cat "$tmp/out")"

# In both shards.
copy
# shellcheck disable=SC2016 # an awk program
edit '{ print } $0 == "1 EditToolbarUITests 208" { print "2 EditToolbarUITests 208" }'
expect 1 "$s" --check
grep -q 'EditToolbarUITests is in more than one shard' "$tmp/out" || fail "both: $(cat "$tmp/out")"

# Listed but gone.
copy
rm "$tmp/ui/OCRResumeUITests.swift"
expect 1 "$s" --check
grep -q 'OCRResumeUITests is in a shard but not under' "$tmp/out" || fail "gone: $(cat "$tmp/out")"

# A shard past the count.
copy
# shellcheck disable=SC2016 # an awk program
edit '$0 == "2 OCRResumeUITests 0" { $1 = 3 } { print }'
expect 1 "$s" --check
grep -q 'shard 3 in scripts/ios-ui-shards.sh' "$tmp/out" || fail "shard 3: $(cat "$tmp/out")"

# What counts as a test class.
copy
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
expect 1 "$s" --check
grep -q 'OpenUITests .*is in no shard' "$tmp/out" || fail "OpenUITests: $(cat "$tmp/out")"
grep -q 'SubclassedTests .*is in no shard' "$tmp/out" || fail "SubclassedTests: $(cat "$tmp/out")"
grep -q 'RegressionCase .*is in no shard' "$tmp/out" || fail "a superclass on the next line: $(cat "$tmp/out")"
grep -q 'GenericCase .*is in no shard' "$tmp/out" || fail "GenericCase: $(cat "$tmp/out")"
! grep -q 'LaunchHelper\|CommentedOutUITests\|func\|var\|shared' "$tmp/out" || fail "not test classes: $(cat "$tmp/out")"
[ "$(grep -c 'is in no shard' "$tmp/out")" = 4 ] || fail "not four new classes: $(cat "$tmp/out")"

echo "ios-ui-shards: ok"
