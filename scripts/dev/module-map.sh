#!/usr/bin/env bash
# Usage: scripts/dev/module-map.sh [--check]
# The desktop app's module map. `just map` prints the module tree under
# apps/desktop/src, one line per module: its name, indented by depth, and
# the first sentence of its `//!` header, the text up to the first ". " or
# the end of the header's first paragraph. `--check`, run by `just lint`,
# fails instead on a module whose first line is not a `//!` header with
# text, naming each one. A module's test-only child, a file named tests.rs,
# needs none; the test modules under a tests/ folder need theirs. The map
# leaves out both, so it shows the app's own modules. Bash 3.2 compatible;
# BWK awk, mawk and gawk. scripts/dev/test-module-map.sh checks it.
# shellcheck source=../lib.sh
source "$(dirname "$0")/../lib.sh"

check=
case "${1:-}" in
  '') ;;
  --check) check=1 ;;
  *) die "usage: scripts/dev/module-map.sh [--check]" ;;
esac
[ $# -le 1 ] || die "usage: scripts/dev/module-map.sh [--check]"

src=apps/desktop/src
[ -d "$src" ] || die "$src not found"

# Paths without `.rs`, the crate root (main) first, then sorted bytewise,
# so a module comes just before its children: `/` sorts before every name
# character.
files=$(cd "$src" && find . -type f -name '*.rs' ! -name tests.rs | sed -e 's|^\./||' -e 's|\.rs$||' | LC_ALL=C sort)
[ -n "$files" ] || die "no modules under $src"
if echo "$files" | grep -qx main; then
  files=$(echo main && { echo "$files" | grep -vx main || true; })
fi

# shellcheck disable=SC2016 # awk fields, not shell ones
echo "$files" | awk -v src="$src" -v check="$check" '
  {
    file = src "/" $0 ".rs"
    first = ""
    sentence = ""
    stop = 0
    n = 0
    while ((getline line < file) > 0) {
      sub(/\r$/, "", line)
      n++
      if (n == 1) first = line
      if (line !~ /^\/\/!/) break
      if (stop) continue
      text = line
      sub(/^\/\/! ?/, "", text)
      if (text ~ /^[ \t]*$/) {
        if (sentence != "") stop = 1
        continue
      }
      sentence = sentence == "" ? text : sentence " " text
      if (match(sentence, /\. /)) {
        sentence = substr(sentence, 1, RSTART)
        stop = 1
      }
    }
    close(file)
    if (first !~ /^\/\/![ \t]*[^ \t]/) {
      missing[++bad] = file
      next
    }
    if (check || $0 ~ /(^|\/)tests\//) next
    depth = split($0, parts, "/")
    name = parts[depth]
    if ($0 == "main") { name = "mdoc"; depth = 0 }
    indent = ""
    for (i = 1; i <= depth; i++) indent = indent "  "
    print indent name ": " sentence
  }
  END {
    for (i = 1; i <= bad; i++) print "module-map: " missing[i] ": no //! header on its first line" > "/dev/stderr"
    if (bad) exit 1
  }
'
