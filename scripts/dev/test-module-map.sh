#!/usr/bin/env bash
# Usage: scripts/dev/test-module-map.sh
# scripts/dev/module-map.sh on scratch trees: `--check` fails on a module
# without a `//!` header on its first line and names it, passes a tree where
# every module has one, and needs none from a tests.rs; the map prints the
# crate root first, each module just above its children and indented by
# depth, with its header's first sentence. Bash 3.2 compatible.
source "$(dirname "$0")/../lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# tree <name>: a scratch repository with the script, since lib.sh moves to
# the root of the repository the script sits in, and a crate root.
tree() {
  rm -rf "${tmp:?}/$1"
  mkdir -p "$tmp/$1/scripts/dev" "$tmp/$1/apps/desktop/src"
  cp scripts/lib.sh "$tmp/$1/scripts/"
  cp scripts/dev/module-map.sh "$tmp/$1/scripts/dev/"
  put "$1" main.rs '//! The app. Its second sentence.
mod a;'
}

# put <name> <path> <text>: writes a module under apps/desktop/src.
put() {
  mkdir -p "$(dirname "$tmp/$1/apps/desktop/src/$2")"
  printf '%s\n' "$3" >"$tmp/$1/apps/desktop/src/$2"
}

# expect <rc> <name> [--check]: runs the script in the tree; its output is
# in $tmp/out.
expect() {
  local rc=0 want=$1 name=$2
  shift 2
  bash "$tmp/$name/scripts/dev/module-map.sh" "$@" >"$tmp/out" 2>&1 || rc=$?
  [ "$rc" = "$want" ] || fail "$name $*: exited $rc, not $want: $(cat "$tmp/out")"
}

# Every module with a header passes, and a tests.rs needs none.
tree good
put good a.rs '//! Owner a.
use b;'
put good a/b.rs '//! The b part of a.'
put good a/tests.rs 'use super::*;'
expect 0 good --check
[ ! -s "$tmp/out" ] || fail "a passing check printed: $(cat "$tmp/out")"

# A module without a header fails, named with its path; so does one whose
# header comes after another line, and one whose first line is an empty
# header.
tree missing
put missing a.rs 'use b;'
expect 1 missing --check
grep -qF 'apps/desktop/src/a.rs: no //! header' "$tmp/out" || fail "the module was not named: $(cat "$tmp/out")"
tree late
put late a.rs '#![allow(dead_code)]
//! Owner a.'
expect 1 late --check
grep -qF 'apps/desktop/src/a.rs:' "$tmp/out" || fail "a late header passed: $(cat "$tmp/out")"
tree empty
put empty a.rs '//!
//! Owner a.'
expect 1 empty --check
tree nested
put nested a.rs '//! Owner a.'
put nested a/deep/c.rs '/// Not a module header.'
expect 1 nested --check
grep -qF 'apps/desktop/src/a/deep/c.rs:' "$tmp/out" || fail "a nested module was not named: $(cat "$tmp/out")"
# Every missing header is named, not only the first.
tree two
put two a.rs 'use b;'
put two z.rs 'use c;'
expect 1 two --check
[ "$(grep -c 'no //! header' "$tmp/out")" = 2 ] || fail "not every missing header was named: $(cat "$tmp/out")"
# A Windows line ending changes nothing.
tree crlf
printf '//! Owner a.\r\nuse b;\r\n' >"$tmp/crlf/apps/desktop/src/a.rs"
expect 0 crlf --check

# The map: the root first as mdoc, then each module before its children,
# indented two spaces a level, with the first sentence only.
tree map
put map a.rs '//! Owner a, which
//! wraps. Then more.'
put map a/b.rs '//! The b part.

//! Not this paragraph.'
put map a_c.rs '//! Beside a.'
put map z.rs '//! Last: no full stop'
put map a/tests.rs 'use super::*;'
expect 0 map
printf '%s\n' 'mdoc: The app.' '  a: Owner a, which wraps.' '    b: The b part.' '  a_c: Beside a.' '  z: Last: no full stop' >"$tmp/want"
diff "$tmp/want" "$tmp/out" >/dev/null || fail "the map: $(diff "$tmp/want" "$tmp/out")"
# The map also fails on a missing header.
put map a_c.rs 'use d;'
expect 1 map
grep -qF 'apps/desktop/src/a_c.rs:' "$tmp/out" || fail "the map passed a missing header: $(cat "$tmp/out")"

# A wrong argument is refused.
tree args
expect 1 args --map

# The repository itself passes.
bash scripts/dev/module-map.sh --check >"$tmp/out" 2>&1 || fail "a desktop module has no header: $(cat "$tmp/out")"

echo "module-map: ok"
