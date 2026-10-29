#!/usr/bin/env bash
# Usage: scripts/test-ci-cache-cleanup.sh
# Runs scripts/ci-cache-cleanup.sh against a fake gh; deletes nothing real.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export GH_LOG="$tmp/gh.log" GH_ENTRIES="$tmp/entries"
mkdir "$tmp/bin"
cat >"$tmp/bin/gh" <<'FAKE'
#!/usr/bin/env bash
set -eu
case "$*" in
  'cache list --ref refs/heads/main --key v0-rust- --sort created_at --order desc --limit 100 --json id,key --jq '*)
    cat "$GH_ENTRIES" ;;
  'cache delete '*) echo "$3" >>"$GH_LOG" ;;
  *) echo "unexpected gh $*" >&2 && exit 90 ;;
esac
FAKE
chmod +x "$tmp/bin/gh"
export PATH="$tmp/bin:$PATH"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# Newest first, as gh lists them: every older entry of the same job and OS
# goes, whether its lock hash or its env hash differs; other jobs stay.
cat >"$GH_ENTRIES" <<'LIST'
6 v0-rust-test-Darwin-arm64-de33f1f6-57c00e6c
5 v0-rust-test-Windows_NT-x64-c34c4943-d4d77f7a
4 v0-rust-test-Darwin-arm64-14cbc3c1-57c00e6c
3 v0-rust-test-Darwin-arm64-de33f1f6-ce98b0bd
2 v0-rust-test-Windows_NT-x64-c34c4943-6c34bede
1 v0-rust-test-Darwin-arm64-14cbc3c1-796622c3
0 v0-rust-clippy-Darwin-arm64-14cbc3c1-ce98b0bd
LIST
: >"$GH_LOG"
scripts/ci-cache-cleanup.sh >"$tmp/out" || fail "cleanup exited non-zero"
[ "$(cat "$GH_LOG")" = $'4\n3\n2\n1' ] || fail "deleted: $(tr '\n' ' ' <"$GH_LOG")"
grep -qx 'deleted 4 older cache generation(s)' "$tmp/out" || fail "summary: $(cat "$tmp/out")"

# No caches, or one generation per key: nothing to delete.
: >"$GH_ENTRIES"
: >"$GH_LOG"
scripts/ci-cache-cleanup.sh >"$tmp/out" || fail "empty list exited non-zero"
[ ! -s "$GH_LOG" ] || fail "deleted from an empty list"
echo 6 v0-rust-test-Darwin-arm64-de33f1f6-57c00e6c >"$GH_ENTRIES"
scripts/ci-cache-cleanup.sh >"$tmp/out" || fail "single entry exited non-zero"
[ ! -s "$GH_LOG" ] || fail "deleted the only generation"

# A failed listing fails the cleanup instead of deleting nothing quietly.
rm "$GH_ENTRIES"
if scripts/ci-cache-cleanup.sh >"$tmp/out" 2>&1; then fail "listing failure ignored"; fi
[ ! -s "$GH_LOG" ] || fail "deleted after a listing failure"
echo "ci-cache-cleanup tests passed"
