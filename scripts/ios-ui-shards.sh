#!/usr/bin/env bash
# Usage: scripts/ios-ui-shards.sh <shard> | --check | --plan | --classes
# The UI test classes (mdocUITests) split over ci-ios.yml's UI parts,
# `ci/ios (ui 1)` and `ci/ios (ui 2)` (ADR 0112), by measured time (ADR
# 0113): `<shard>` prints that shard's classes as xcodebuild `-only-testing`
# identifiers on one line (`mdocUITests/EditToolbarUITests ...`), which
# `just ios-test` takes as its `only` arguments. `--check` prints nothing;
# `--plan` prints each class's shard and seconds (`1 EditModeUITests 146`,
# `median` after a class the table lacks) and each shard's total;
# `--classes` prints the test classes, one per line.
#
# The times are apps/ios/ui-class-times.tsv's (MDOC_UI_CLASS_TIMES replaces
# it): `<class>\t<seconds>[\t<parts>]` lines and `#` comments, measured from
# green UI parts and refreshed by the nightly (scripts/ios-ui-class-times.sh).
# Greedy, longest first: each class, from the longest (by name on a tie),
# goes to the shard with the least time so far (then the fewest classes,
# then the lower number). A class the table lacks, such as a new one, counts
# as the table's median; a row for a class that is gone counts for the
# median only. The timing tests (LivePreviewUITests', `timing: true`) add
# about 4 minutes to their shard each night, which the table leaves out.
#
# It fails, naming it, when a test class is declared twice under
# apps/ios/mdocUITests, or the table repeats a class, has a row that is not
# a class and seconds, or is missing. A test class is a Swift class there
# that inherits XCTestCase or whose name ends in `Tests`. Bash 3.2
# compatible.
source "$(dirname "$0")/lib.sh"

count=2
dir=${MDOC_UI_TESTS_DIR:-apps/ios/mdocUITests}
times=${MDOC_UI_CLASS_TIMES:-apps/ios/ui-class-times.tsv}
usage="usage: scripts/ios-ui-shards.sh <shard 1-$count> | --check | --plan | --classes"

case "${1:-}" in
  --check | --plan | --classes) want=$1 ;;
  [1-9])
    [ "$1" -le "$count" ] || die "no shard $1: there are $count"
    want=$1
    ;;
  *) die "$usage" ;;
esac
[ "$#" -eq 1 ] || die "$usage"
[ -d "$dir" ] || die "no $dir"
[ -f "$times" ] || die "no $times"

# The test classes declared under the folder, one per line. Each file is
# read as one line without its `//` comments, so a declaration whose
# superclass is on a later line (`class A:` then `XCTestCase {`) counts:
# what follows the name up to its `{` is its generic clause and
# inheritance.
# shellcheck disable=SC2016 # awk fields, not shell ones
found=$(find "$dir" -type f -name '*.swift' -print0 | xargs -0 awk '
  function scan(t, decl, head, i) {
    while (match(t, /(^|[^A-Za-z0-9_])class[ \t]+[A-Za-z_][A-Za-z0-9_]*/)) {
      decl = substr(t, RSTART, RLENGTH)
      sub(/^.*class[ \t]+/, "", decl)
      t = substr(t, RSTART + RLENGTH)
      # `class func`, `class var` and the like declare no class.
      if (decl ~ /^(func|var|let|subscript|static|final|override|open|public|internal|private|fileprivate)$/) continue
      head = t
      i = index(head, "{")
      if (i) head = substr(head, 1, i - 1)
      if ((head ~ /^[ \t]*(<[^>]*>)?[ \t]*:/ && head ~ /XCTestCase/) || decl ~ /Tests$/) print decl
    }
  }
  FNR == 1 && NR > 1 { scan(text); text = "" }
  {
    line = $0
    sub(/\/\/.*/, "", line)
    text = text " " line
  }
  END { scan(text) }' | LC_ALL=C sort)

bad=0
for c in $(printf '%s\n' "$found" | uniq -d); do
  echo "error: $c is declared more than once under $dir" >&2
  bad=1
done
# The table's rows as `<class> <seconds>`; a bad row is an error line.
# shellcheck disable=SC2016 # awk fields, not shell ones
rows=$(awk -F '\t' '
  { sub(/\r$/, "") }
  /^#/ || /^[ \t]*$/ { next }
  $1 !~ /^[A-Za-z_][A-Za-z0-9_]*$/ || $2 !~ /^[0-9]+(\.[0-9]+)?$/ {
    printf "error: %s:%d is not <class><tab><seconds>: %s\n", FILENAME, FNR, $0
    next
  }
  { print $1, $2 }' "$times")
if printf '%s\n' "$rows" | grep '^error: ' >&2; then bad=1; fi
rows=$(printf '%s\n' "$rows" | grep -v '^error: ' || true)
for c in $(printf '%s\n' "$rows" | awk 'NF { print $1 }' | LC_ALL=C sort | uniq -d); do
  echo "error: $c is in $times more than once" >&2
  bad=1
done
[ "$bad" = 0 ] || exit 1
[ -n "$found" ] || die "no test class under $dir"

if [ "$want" = --classes ]; then
  printf '%s\n' "$found"
  exit 0
fi

# Each class with its seconds (`median` marks a guess), longest first, then
# packed: `<shard> <class> <seconds> [median]`.
# shellcheck disable=SC2016 # awk fields, not shell ones
plan=$(
  {
    printf '%s\n' "$rows" | awk 'NF { print "row", $1, $2 }'
    printf '%s\n' "$found" | awk '{ print "class", $1 }'
  } | awk '
    $1 == "row" { t[$2] = $3; v[++n] = $3 + 0; next }
    { c[++m] = $2 }
    END {
      # The median of the rows: insertion sort, as they are few.
      for (i = 2; i <= n; i++) {
        x = v[i]
        for (j = i - 1; j >= 1 && v[j] > x; j--) v[j + 1] = v[j]
        v[j + 1] = x
      }
      med = n ? (n % 2 ? v[(n + 1) / 2] : (v[n / 2] + v[n / 2 + 1]) / 2) : 0
      for (i = 1; i <= m; i++) {
        if (c[i] in t) print c[i], t[c[i]] + 0
        else print c[i], med, "median"
      }
    }' | LC_ALL=C sort -k2,2nr -k1,1 | awk -v count="$count" '
    {
      s = 1
      for (k = 2; k <= count; k++)
        if (tot[k] < tot[s] || (tot[k] == tot[s] && num[k] < num[s])) s = k
      tot[s] += $2
      num[s]++
      print s, $0
    }'
)

case "$want" in
  --check) exit 0 ;;
  --plan)
    printf '%s\n' "$plan"
    # shellcheck disable=SC2016 # awk fields, not shell ones
    printf '%s\n' "$plan" | awk -v count="$count" '
      { tot[$1] += $3; num[$1]++ }
      END { for (s = 1; s <= count; s++) printf "shard %d: %d classes, %d s\n", s, num[s], tot[s] }'
    exit 0
    ;;
esac
only=$(printf '%s\n' "$plan" | awk -v s="$want" '$1 == s { printf "%smdocUITests/%s", (n++ ? " " : ""), $2 }')
[ -n "$only" ] || die "shard $want has no classes"
echo "$only"
