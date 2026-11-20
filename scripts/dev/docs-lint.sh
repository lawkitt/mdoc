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
#     (scripts/dev/docs-lint-pending.md) keeps its older ceiling until its
#     #981 PR trims it, and fails once it fits the new one, so the list only
#     shrinks; a pending ADR is held to its older count, which also left out
#     References and Terms paragraphs. A pending row must name an existing
#     file and a number;
#   - ADR numbers against docs/adr/README.md: its "Last number: NNNN" line
#     must be at or above every ADR file, and every number up to it must be
#     either one file or on its "Retired:" line (NNNN or NNNN-NNNN items),
#     never both, so a retired number is never reused;
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

pending_list=scripts/dev/docs-lint-pending.md
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
check() { # file ceiling count [count under the older rule]
  local old
  old=$(pending "$1")
  if [ -n "$old" ]; then
    if [ "$3" -le "$2" ]; then
      err "$1 fits its ceiling of $2 now: take it off docs-lint's pending list"
    elif [ "${4:-$3}" -gt "$old" ]; then
      err "$1: ${4:-$3} words, over its pending ceiling of $old (the target is $2)"
    fi
  elif [ "$3" -gt "$2" ]; then
    err "$1: $3 words, over its ceiling of $2"
  fi
}

# The pending list's rows.
if [ -f "$pending_list" ]; then
  { grep -v '^#' "$pending_list" || true; } | while read -r path limit rest; do
    [ -n "$path" ] || continue
    [ -f "$path" ] || echo "$pending_list: $path is not a file"
    case "$limit" in '' | *[!0-9]*) echo "$pending_list: $path needs a number, not '$limit'" ;; esac
    [ -z "$rest" ] || echo "$pending_list: $path has more than a path and a number"
  done >"$tmp/rows"
  while IFS= read -r line; do err "$line"; done <"$tmp/rows"
fi

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
  older=$(awk '/^(Status|References|Terms):/ { skip = 1 } /^[ \t]*$/ { skip = 0 } !skip { w += NF } END { print w + 0 }' "$f")
  check "$f" 250 "$n" "$older"
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
  if [ -z "$last" ]; then
    err "docs/adr/README.md has no 'Last number: NNNN' line"
  else
    # Retired numbers, one per line, from items like 0003 or 0006-0016.
    sed -n 's/^Retired: //p' docs/adr/README.md | tr ',' '\n' | tr -d ' .' | awk -F- '
      /^[0-9][0-9][0-9][0-9]$/ { print $1; next }
      /^[0-9][0-9][0-9][0-9]-[0-9][0-9][0-9][0-9]$/ { for (i = $1 + 0; i <= $2 + 0; i++) printf "%04d\n", i; next }
      NF { print "bad " $0 }
    ' >"$tmp/retired"
    awk -v last="$last" '
      FILENAME == ARGV[1] { if ($0 ~ /^bad /) bad[substr($0, 5)] = 1; else retired[$1] = 1; next }
      { file[$1] = 1 }
      END {
        for (b in bad) printf "docs/adr/README.md: Retired item \"%s\" is not NNNN or NNNN-NNNN\n", b
        for (n in file) if (n + 0 > last + 0) printf "ADR %s is above docs/adr/README.md'"'"'s Last number (%s): raise it\n", n, last
        for (i = 1; i <= last + 0; i++) {
          n = sprintf("%04d", i)
          if ((n in file) && (n in retired)) printf "ADR %s is a file and Retired: a retired number is never reused\n", n
          else if (!(n in file) && !(n in retired)) printf "ADR %s is neither a file nor Retired in docs/adr/README.md\n", n
        }
      }
    ' "$tmp/retired" "$tmp/numbers" | sort >"$tmp/gaps"
    while IFS= read -r line; do err "$line"; done <"$tmp/gaps"
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
