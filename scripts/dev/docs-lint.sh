#!/usr/bin/env bash
# Usage: scripts/dev/docs-lint.sh
# The docs' shape (docs/development.md, Writing docs), run by `just lint`. It
# fails on:
#   - a file over its word ceiling (`wc -w` words): docs/development.md 1,500,
#     not counting its ```toml fleet-profile block; docs/specs/ci.md 500 and
#     every other spec 1,500; each ADR 250, not counting its Status paragraph;
#     CONTEXT.md 2,000, and 60 per entry (a `**Term**` line up to the next
#     entry or heading); every tracked README.md 800, except the
#     repository's own and vendor/. A file on the pending list
#     (scripts/dev/docs-lint-pending.txt) keeps its older ceiling until its
#     #981 PR trims it, and fails once it fits the new one, so the list only
#     shrinks;
#   - an ADR number used by two files, or docs/adr/README.md without a
#     "Last number: NNNN" line at or above every ADR file's number;
#   - a docs/design/ folder: a grill's record goes in its PR or an issue;
#   - a dated (YYYY-MM-DD…) or pr-* folder anywhere under docs/evidence/;
#   - a changelog fragment (changes/, changes/ios/, changes/android/; not
#     their README.md) that `just release` would reject: a name outside
#     letters, digits, '.', '_' and '-', a first line that is not a '- '
#     bullet, or top-level fragments with no `## Unreleased` in CHANGELOG.md.
# Reads no history: CI clones one commit. Bash 3.2 compatible; BWK awk, mawk
# and gawk. scripts/dev/test-docs-lint.sh checks it.
# shellcheck source=../lib.sh
source "$(dirname "$0")/../lib.sh"

[ $# -eq 0 ] || die "usage: scripts/dev/docs-lint.sh"

pending_list=scripts/dev/docs-lint-pending.txt
pending() { # path: its older ceiling, if it is on the pending list
  [ -f "$pending_list" ] || return 0
  awk -v f="$1" '$1 == f { print $2; exit }' "$pending_list"
}

bad=0
err() {
  echo "docs-lint: $*" >&2
  bad=1
}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

words() { wc -w <"$1" | tr -d ' '; }
check() { # file ceiling count
  local old
  old=$(pending "$1")
  if [ -n "$old" ]; then
    if [ "$3" -le "$2" ]; then
      err "$1 fits its ceiling of $2 now: take it off docs-lint's pending list"
    elif [ "$3" -gt "$old" ]; then
      err "$1: $3 words, over its pending ceiling of $old (the target is $2)"
    fi
  elif [ "$3" -gt "$2" ]; then
    err "$1: $3 words, over its ceiling of $2"
  fi
}

# Ceilings.
if [ -f docs/development.md ]; then
  n=$(awk '/^```toml fleet-profile/ { fenced = 1 } !fenced { w += NF } fenced && /^```$/ { fenced = 0 } END { print w + 0 }' docs/development.md)
  check docs/development.md 1500 "$n"
fi
for f in docs/specs/*.md; do
  [ -f "$f" ] || continue
  [ "${f##*/}" = README.md ] && continue
  limit=1500
  [ "$f" = docs/specs/ci.md ] && limit=500
  check "$f" "$limit" "$(words "$f")"
done
for f in docs/adr/[0-9][0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] || continue
  n=$(awk '/^Status:/ { skip = 1 } /^[ \t]*$/ { skip = 0 } !skip { w += NF } END { print w + 0 }' "$f")
  check "$f" 250 "$n"
done
if [ -f CONTEXT.md ]; then
  check CONTEXT.md 2000 "$(words CONTEXT.md)"
  awk '
    function done_entry() { if (term != "" && w > 60) printf "%s\t%d\n", term, w }
    /^\*\*[^*]+\*\*/ { done_entry(); term = $0; sub(/^\*\*/, "", term); sub(/\*\*.*/, "", term); w = 0 }
    /^#/ { done_entry(); term = ""; next }
    term != "" { w += NF }
    END { done_entry() }
  ' CONTEXT.md >"$tmp/entries"
  while IFS="$(printf '\t')" read -r term n; do
    err "CONTEXT.md: \"$term\" is $n words, over 60 an entry"
  done <"$tmp/entries"
fi
git ls-files -- '*README.md' 2>/dev/null >"$tmp/readmes" || true
while IFS= read -r f; do
  case "$f" in
    README.md | vendor/*) continue ;;
  esac
  [ -f "$f" ] && check "$f" 800 "$(words "$f")"
done <"$tmp/readmes"

# ADR numbers.
: >"$tmp/numbers"
for f in docs/adr/[0-9][0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] && basename "$f" | cut -c1-4 >>"$tmp/numbers"
done
sort "$tmp/numbers" | uniq -d | while IFS= read -r n; do
  echo "ADR $n has more than one file"
done >"$tmp/dupes"
while IFS= read -r line; do err "$line"; done <"$tmp/dupes"
if [ -s "$tmp/numbers" ]; then
  last=$(sed -n 's/^Last number: \([0-9][0-9][0-9][0-9]\)\.*$/\1/p' docs/adr/README.md 2>/dev/null | head -1)
  high=$(sort "$tmp/numbers" | tail -1)
  if [ -z "$last" ]; then
    err "docs/adr/README.md has no 'Last number: NNNN' line"
  elif [ "$((10#$high))" -gt "$((10#$last))" ]; then
    err "ADR $high is above docs/adr/README.md's Last number ($last): raise it"
  fi
fi

# Changelog fragments, by release.sh's rules, so a bad one fails here rather
# than at `just release`. find, as release.sh does: a glob would miss a
# dot-named one.
top=0
for d in changes changes/ios changes/android; do
  if [ -d "$d" ]; then find "$d" -maxdepth 1 -type f -name '*.md'; fi
done | LC_ALL=C sort >"$tmp/fragments"
while IFS= read -r f; do
  case "$f" in */README.md) continue ;; esac
  case "$f" in changes/*/*) ;; *) top=1 ;; esac
  name=${f##*/}
  case "${name%.md}" in
    '' | *[!A-Za-z0-9._-]*) err "$f: name fragments with letters, digits, '.', '_' and '-'" ;;
  esac
  [ "$(head -c 2 "$f")" = '- ' ] || err "$f must start with a '- ' CHANGELOG bullet"
done <"$tmp/fragments"
if [ "$top" = 1 ] && ! grep -q '^## Unreleased' CHANGELOG.md 2>/dev/null; then
  err "CHANGELOG.md has no ## Unreleased section for changes/"
fi

# No design records, and no dated or per-PR evidence.
[ -e docs/design ] && err "docs/design/ exists: a grill's record goes in its PR or an issue"
if [ -d docs/evidence ]; then
  find docs/evidence -type d \( -name '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]*' -o -name 'pr-*' \) |
    sort >"$tmp/evidence"
  while IFS= read -r d; do
    err "$d: a dated or pr-* evidence folder; images go in the PR, results in an issue"
  done <"$tmp/evidence"
fi

[ "$bad" -eq 0 ] || exit 1
echo "docs-lint: ok"
