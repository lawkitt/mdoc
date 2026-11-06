#!/usr/bin/env bash
# Usage: scripts/ios-core-cache.sh key | check
# The cache of the built iOS core (#631): what `just ios-core` puts in
# target/ios, which main-caches.yml saves on main and ci-ios.yml restores to
# skip that build on an exact hit.
#
#   key    Prints the cache's `key=` and `paths=` for $GITHUB_OUTPUT. The key
#          hashes the committed tree of every input below and the pinned
#          Xcode (XCODE_VERSION in scripts/tools.env, which links and
#          assembles the xcframeworks), so a change to any of them misses.
#          Refuses uncommitted changes to the inputs: the key is HEAD's.
#   check  After `just ios-core`: fails when the build read a file the key
#          does not hash. Cargo's dep-info for the core (both targets) and
#          for uniffi-bindgen lists every workspace file rustc and the build
#          scripts read; each must be a committed file under an input.
#          Registry and git crates are pinned by Cargo.lock; target/ holds
#          only what the build itself wrote.
#
# Besides the crates, the build reads only the `ios-core` recipe (justfile),
# build-ios-core.sh, lib.sh and the script's checksum-pinned downloads; dep-info
# cannot see those, so a new file the recipe or the script reads goes in
# `inputs` by hand.
# Build.xcconfig (scripts/ios-build-info.sh) names the commit, so it is not
# cached: ci-ios.yml writes it after a restore.
source "$(dirname "$0")/lib.sh"

inputs=(crates resources Cargo.toml Cargo.lock rust-toolchain.toml .cargo justfile
  scripts/build-ios-core.sh scripts/lib.sh scripts/ios-core-cache.sh)
# What the app's build reads from target/ios, and the licences beside it.
outputs=(target/ios/MdocCore.xcframework target/ios/pdfium.xcframework target/ios/swift target/ios/licenses)
# Bump to drop every saved core, for a change the inputs cannot show.
version=v1

key() {
  git diff --quiet HEAD -- "${inputs[@]}" ||
    die "uncommitted changes to the iOS core's inputs; its key hashes HEAD"
  [ -z "$(git ls-files --others --exclude-standard -- "${inputs[@]}")" ] ||
    die "untracked files among the iOS core's inputs; its key hashes HEAD"
  local xcode hash
  xcode=$(sed -n 's/^XCODE_VERSION=//p' scripts/tools.env)
  [ -n "$xcode" ] || die "no XCODE_VERSION in scripts/tools.env"
  hash=$({
    git ls-tree -r HEAD -- "${inputs[@]}"
    echo "XCODE_VERSION=$xcode"
  } | shasum -a 256 | cut -c1-64)
  echo "key=ios-core-$version-$hash"
  echo "paths<<PATHS_END"
  printf '%s\n' "${outputs[@]}"
  echo "PATHS_END"
}

check() {
  local depinfo=(target/aarch64-apple-ios/mobile/libmdoc_ffi.d
    target/aarch64-apple-ios-sim/mobile/libmdoc_ffi.d target/debug/uniffi-bindgen.d)
  local f tracked
  for f in "${depinfo[@]}"; do
    [ -f "$f" ] || die "no $f: run just ios-core first"
  done
  tracked=$(git ls-files -- "${inputs[@]}")
  python3 - "$PWD" "${CARGO_HOME:-$HOME/.cargo}" <(printf '%s\n' "$tracked") "${depinfo[@]}" <<'PY' ||
import os, re, sys
root, cargo_home = os.path.realpath(sys.argv[1]), os.path.realpath(sys.argv[2])
tracked = set(open(sys.argv[3], encoding="utf-8").read().splitlines())
unhashed = set()
for name in sys.argv[4:]:
    for line in open(name, encoding="utf-8"):
        # `<output>: <dep> <dep> ...`, spaces in paths escaped as `\ `.
        parts = re.split(r"(?<!\\): ", line.rstrip("\n"), maxsplit=1)
        if line.startswith("#") or len(parts) < 2:
            continue
        for dep in re.split(r"(?<!\\) +", parts[1].strip()):
            if not dep:
                continue
            path = os.path.realpath(dep.replace("\\ ", " "))
            rel = os.path.relpath(path, root)
            if rel == "target" or rel.startswith("target" + os.sep):
                continue
            if rel.startswith(".." + os.sep):
                if not path.startswith(cargo_home + os.sep):
                    unhashed.add(path)
            elif rel not in tracked:
                unhashed.add(rel)
for path in sorted(unhashed):
    print(f"  {path}", file=sys.stderr)
sys.exit(1 if unhashed else 0)
PY
    die "the iOS core build read the files above, which its cache key does not hash: add them to inputs in scripts/ios-core-cache.sh"
  echo "ios-core-cache: the key hashes every file the build read"
}

case "${1:-}" in
  key) key ;;
  check) check ;;
  *) die "usage: scripts/ios-core-cache.sh key | check" ;;
esac
