#!/usr/bin/env bash
# Usage: scripts/quarantine.sh check|filter|list [file]
#
# The quarantine list (ADR 0095), .config/quarantine.txt by default: lines of
# `<nextest filter>  #<issue>  <expires YYYY-MM-DD>`, fields separated by two
# or more spaces; `#` lines and blank lines are comments.
#
#   check   fail on a malformed line or one past its date (QUARANTINE_TODAY,
#           YYYY-MM-DD, overrides today, for the suite)
#   filter  print one nextest filterset matching every listed test, or nothing
#   list    print the listed lines, one per line, as `filter|issue|expires`
#
# Bash 3.2 compatible (macOS /bin/bash).
# shellcheck source=lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

cmd=${1:-}
file=${2:-.config/quarantine.txt}

# Prints `filter|issue|expires` per entry; fails on a malformed line.
entries() {
  [ -f "$file" ] || return 0
  awk -v file="$file" '
    /^[[:space:]]*(#|$)/ { next }
    {
      line = $0
      sub(/[[:space:]]+$/, "", line)
      n = split(line, f, /[[:space:]][[:space:]]+/)
      if (n != 3 || f[2] !~ /^#[0-9]+$/ || f[3] !~ /^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/) {
        printf "error: %s:%d: expected `<nextest filter>  #<issue>  <expires YYYY-MM-DD>`: %s\n", file, NR, $0 > "/dev/stderr"
        bad = 1
        next
      }
      print f[1] "|" f[2] "|" f[3]
    }
    END { exit bad }' "$file"
}

case "$cmd" in
  check)
    today=${QUARANTINE_TODAY:-$(date -u +%Y-%m-%d)}
    list=$(entries) || exit 1
    rc=0
    while IFS='|' read -r filter issue expires; do
      [ -n "$filter" ] || continue
      if [[ $expires < $today ]]; then
        echo "error: quarantine of $filter ($issue) expired on $expires: fix the test and remove the line, or renew it in a PR that says why" >&2
        rc=1
      fi
    done <<<"$list"
    exit "$rc"
    ;;
  filter)
    list=$(entries) || exit 1
    out=''
    while IFS='|' read -r filter _ _; do
      [ -n "$filter" ] || continue
      out="${out:+$out | }($filter)"
    done <<<"$list"
    [ -z "$out" ] || echo "$out"
    ;;
  list) entries ;;
  *) die "usage: scripts/quarantine.sh check|filter|list [file]" ;;
esac
