#!/usr/bin/env bash
# Usage: scripts/dev/docs-lint.sh [--check]
# The docs' shape (ADR 0118), checked by `just lint` with --check. It fails on:
#   - a file over its word budget: docs/development.md 2,500; docs/specs/ci.md
#     800 and every other spec 3,000; each ADR 400, not counting its Status,
#     References and Terms paragraphs; CONTEXT.md 3,000, and 60 per entry (a
#     `**Term**` line up to the next entry or heading); every tracked
#     README.md 1,500, except the repository's own, vendor/ and the generated
#     docs/adr/README.md (`wc -w` words);
#   - a docs/design/ folder: a grill's record goes in its PR or an issue;
#   - a dated (YYYY-MM-DD…) or pr-* folder anywhere under docs/evidence/;
#   - an ADR number from 0001 to the highest one that is neither a file in
#     docs/adr/ nor claimed by exactly one `Folds:` line, or that is both. A
#     `Folds:` line (under docs/, outside docs/adr/) lists 4-digit numbers and
#     may wrap until a blank line;
#   - with --check, a stale docs/adr/README.md: its rows must name each number
#     once, as the kept file or the file whose `Folds:` line claims it.
# Without --check it writes docs/adr/README.md first: a row per number with
# its title, and for a folded number the claiming file and a permalink to its
# last full text in git history. A folded row already in the index keeps its
# title and permalink, so only new folds read history (a shallow clone cannot).
# --check reads no history: CI clones one commit. Bash 3.2 compatible; BWK
# awk, mawk and gawk. scripts/dev/test-docs-lint.sh checks it.
# shellcheck source=../lib.sh
source "$(dirname "$0")/../lib.sh"

mode=index
case "${1:-}" in
  --check) mode=check ;;
  '') ;;
  *) die "usage: scripts/dev/docs-lint.sh [--check]" ;;
esac

index=docs/adr/README.md
repo_url=https://github.com/lawkitt/mdoc
bad=0
err() {
  echo "docs-lint: $*" >&2
  bad=1
}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

words() { wc -w <"$1" | tr -d ' '; }
over() { # file limit count
  [ "$3" -le "$2" ] || err "$1: $3 words, over its budget of $2"
}

# Budgets.
[ -f docs/development.md ] && over docs/development.md 2500 "$(words docs/development.md)"
for f in docs/specs/*.md; do
  [ -f "$f" ] || continue
  limit=3000
  [ "$f" = docs/specs/ci.md ] && limit=800
  over "$f" "$limit" "$(words "$f")"
done
for f in docs/adr/[0-9][0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] || continue
  n=$(awk '/^(Status|References|Terms):/ { skip = 1 } /^[ \t]*$/ { skip = 0 } !skip { w += NF } END { print w + 0 }' "$f")
  over "$f" 400 "$n"
done
if [ -f CONTEXT.md ]; then
  over CONTEXT.md 3000 "$(words CONTEXT.md)"
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
    README.md | "$index" | vendor/*) continue ;;
  esac
  [ -f "$f" ] && over "$f" 1500 "$(words "$f")"
done <"$tmp/readmes"

# No design records, and no dated or per-PR evidence.
[ -e docs/design ] && err "docs/design/ exists: a grill's record goes in its PR or an issue"
if [ -d docs/evidence ]; then
  find docs/evidence -type d \( -name '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]*' -o -name 'pr-*' \) |
    sort >"$tmp/evidence"
  while IFS= read -r d; do
    err "$d: a dated or pr-* evidence folder; images go in the PR, results in an issue"
  done <"$tmp/evidence"
fi

# ADR numbers: "<number>\t<file>" for each kept file and each claim.
: >"$tmp/files"
for f in docs/adr/[0-9][0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] && printf '%s\t%s\n' "$(basename "$f" | cut -c1-4)" "$f" >>"$tmp/files"
done
find docs -name '*.md' ! -path 'docs/adr/*' | sort | while IFS= read -r f; do
  awk -v file="$f" '
    function claims(line) {
      while (match(line, /[0-9][0-9][0-9][0-9]/)) {
        printf "%s\t%s\n", substr(line, RSTART, 4), file
        line = substr(line, RSTART + 4)
      }
    }
    /^Folds:/ { on = 1; claims(substr($0, 7)); next }
    on && /^[ \t]*$/ { on = 0 }
    on { claims($0) }
  ' "$f"
done >"$tmp/claims"
cut -f1 "$tmp/files" | sort | uniq -d | while IFS= read -r n; do
  echo "ADR $n has more than one file"
done >"$tmp/numbers"
cut -f1 "$tmp/claims" | sort | uniq -d | while IFS= read -r n; do
  echo "ADR $n is claimed by more than one Folds: line ($(awk -v n="$n" '$1 == n { print $2 }' "$tmp/claims" | tr '\n' ' ' | sed 's/ $//'))"
done >>"$tmp/numbers"
max=$(cut -f1 "$tmp/files" "$tmp/claims" | sort | tail -1)
max=${max:-0}
n=1
while [ "$n" -le "$((10#$max))" ]; do
  num=$(printf '%04d' "$n")
  file=$(awk -v n="$num" '$1 == n { print $2; exit }' "$tmp/files")
  claim=$(awk -v n="$num" '$1 == n { print $2; exit }' "$tmp/claims")
  if [ -n "$file" ] && [ -n "$claim" ]; then
    echo "ADR $num is a file and claimed by $claim: delete one"
  elif [ -z "$file" ] && [ -z "$claim" ]; then
    echo "ADR $num is neither a file in docs/adr/ nor claimed by a Folds: line"
  fi
  if [ -n "$file" ]; then
    printf '%s\tkept\t%s\n' "$num" "$(basename "$file")"
  elif [ -n "$claim" ]; then
    case "$claim" in
      docs/*) target="../${claim#docs/}" ;;
      *) target="../../$claim" ;;
    esac
    printf '%s\tfolded\t%s\n' "$num" "$target"
  fi >>"$tmp/expected"
  n=$((n + 1))
done >>"$tmp/numbers"
: >>"$tmp/expected"
while IFS= read -r line; do err "$line"; done <"$tmp/numbers"

# The index's rows as "<number>\t<kind>\t<target>\t<title>\t<permalink>".
index_rows() {
  [ -f "$index" ] || return 0
  awk -F'|' '
    $2 ~ /^ [0-9][0-9][0-9][0-9] $/ {
      num = $2; gsub(/ /, "", num)
      title = $3; sub(/^ /, "", title); sub(/ $/, "", title)
      where = $4
      if (where ~ /^ Kept/) {
        target = title; sub(/^.*\]\(/, "", target); sub(/\).*$/, "", target)
        sub(/^\[/, "", title); sub(/\]\(.*$/, "", title)
        printf "%s\tkept\t%s\t%s\t\n", num, target, title
      } else {
        target = where; sub(/^[^(]*\(/, "", target); sub(/\).*$/, "", target)
        link = where; sub(/^.*\[last text\]\(/, "", link); sub(/\).*$/, "", link)
        printf "%s\tfolded\t%s\t%s\t%s\n", num, target, title, link
      }
    }
  ' "$index"
}

if [ "$mode" = check ]; then
  index_rows | cut -f1-3 >"$tmp/actual"
  if ! cmp -s "$tmp/expected" "$tmp/actual"; then
    err "$index is stale: run scripts/dev/docs-lint.sh"
    diff "$tmp/expected" "$tmp/actual" | sed -n 's/^\([<>]\)/  \1/p' | head -20 >&2
  fi
else
  index_rows >"$tmp/old"
  {
    echo "# Architecture decisions"
    echo
    echo "Generated by \`scripts/dev/docs-lint.sh\` from the ADR files and the"
    echo "specs' \`Folds:\` lines; don't edit it by hand. A kept ADR is a file"
    echo "here. A folded one lives on in the file that claims it, and its last"
    echo "full text is in git history."
    echo
    echo "| ADR | Title | Where |"
    echo "|---|---|---|"
    while IFS="$(printf '\t')" read -r num kind target; do
      if [ "$kind" = kept ]; then
        title=$(sed -n 's/^# //p' "docs/adr/$target" | head -1)
        echo "| $num | [$title]($target) | Kept |"
        continue
      fi
      title=$(awk -F'\t' -v n="$num" -v t="$target" '$1 == n && $2 == "folded" && $3 == t && $5 != "" { print $4; exit }' "$tmp/old")
      link=$(awk -F'\t' -v n="$num" -v t="$target" '$1 == n && $2 == "folded" && $3 == t && $5 != "" { print $5; exit }' "$tmp/old")
      if [ -z "$link" ]; then
        # The file as it last was: still in HEAD (deleted in the work tree),
        # or before the commit that deleted it.
        path=$(git ls-tree --name-only HEAD docs/adr/ 2>/dev/null | grep "^docs/adr/$num-" | head -1 || true)
        if [ -n "$path" ]; then
          last=$(git log -1 --format=%H -- "$path" 2>/dev/null || true)
        else
          path=$(git log --diff-filter=D --name-only --format= -- "docs/adr/$num-*.md" 2>/dev/null | grep -m1 . || true)
          last=
          if [ -n "$path" ]; then
            gone=$(git log -1 --format=%H --diff-filter=D -- "$path")
            last=$(git log -1 --format=%H "$gone^" -- "$path" 2>/dev/null || true)
          fi
        fi
        if [ -z "$last" ]; then
          err "ADR $num: no earlier text in git history (a shallow clone?)"
          continue
        fi
        title=$(git show "$last:$path" | sed -n 's/^# //p' | head -1)
        link="$repo_url/blob/$last/$path"
      fi
      echo "| $num | $title | [${target#../}]($target) · [last text]($link) |"
    done <"$tmp/expected"
  } >"$tmp/index"
  cp "$tmp/index" "$index"
fi

[ "$bad" -eq 0 ] || exit 1
[ "$mode" = check ] && echo "docs-lint: ok" || echo "docs-lint: ok, wrote $index"
