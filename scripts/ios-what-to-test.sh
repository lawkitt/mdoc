#!/usr/bin/env bash
# Usage: scripts/ios-what-to-test.sh check <file>
#        scripts/ios-what-to-test.sh from-tag <tag>
# A TestFlight build's What to Test (ADR 0107). `check` fails unless the text
# opens with the never-client-files line, fits TestFlight's 4,000
# characters, has no `---` line (which ends it in a tag's annotation) and no
# symbol App Store Connect refuses in What to Test: emoji, the Miscellaneous
# Symbols and Dingbats blocks (it refused ☰ in rc.3, 2026-11-08), Misc
# Symbols and Arrows, variation selectors and the zero-width joiner.
# `from-tag` prints an `ios-v*` tag's What to Test: its annotation's body up
# to the `---` line, below which the gate's trailers sit. Bash 3.2.
source "$(dirname "$0")/lib.sh"

opening="Test with documents you are allowed to share, never client files; check a screenshot before you send it."
limit=4000
usage="usage: scripts/ios-what-to-test.sh check <file> | from-tag <tag>"

check() {
  local file=$1 first count refused
  [ -s "$file" ] || die "What to Test: $file is missing or empty"
  first=$(grep -m1 -v '^[[:space:]]*$' "$file" || true)
  case "$first" in
    "$opening"*) ;;
    *) die "What to Test must open with: $opening" ;;
  esac
  ! grep -qx -- '---' "$file" || die "What to Test has a '---' line, which ends it in the tag"
  # Characters, not bytes: perl reads UTF-8 on macOS and Linux alike.
  count=$(perl -CSD -0777 -ne 'print length' "$file")
  [ "$count" -le "$limit" ] || die "What to Test has $count characters; TestFlight takes $limit"
  refused=$(perl -CSD -0777 -ne 'my %seen; print join " ", grep { !$seen{$_}++ } /([\x{2600}-\x{27BF}\x{2B00}-\x{2BFF}\x{FE00}-\x{FE0F}\x{200D}\x{1F000}-\x{1FAFF}])/g' "$file")
  [ -z "$refused" ] || die "What to Test has characters App Store Connect refuses: $refused"
}

from_tag() {
  local tag=$1 type
  type=$(git cat-file -t "refs/tags/$tag" 2>/dev/null) || die "no tag $tag"
  [ "$type" = tag ] || die "$tag is not an annotated tag"
  git for-each-ref --format='%(contents:body)' "refs/tags/$tag" |
    awk '$0 == "---" { exit } { print }' |
    perl -0777 -pe 's/\s+\z/\n/'
}

case "${1:-}" in
  check) check "${2:?$usage}" ;;
  from-tag) from_tag "${2:?$usage}" ;;
  *) die "$usage" ;;
esac
