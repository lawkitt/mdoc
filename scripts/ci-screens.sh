#!/usr/bin/env bash
# Usage: scripts/ci-screens.sh <renders-dir> <out-dir> <globs>
# Copies the render test's PNGs whose path under <renders-dir> matches one of
# the comma-separated <globs> (`*` also matches `/`, so `*settings*` finds
# both themes) into <out-dir>, keeping their paths: the small screens
# artifact of the desktop CI job, for a reviewer who wants a few screens, not
# the full set (ADR 0095). Fails when nothing matches, so a mistyped glob is
# not an empty artifact. Relative paths are from the repository root. Bash 3.2
# compatible (macOS /bin/bash).
source "$(dirname "$0")/lib.sh"

[ "$#" -eq 3 ] || die "usage: scripts/ci-screens.sh <renders-dir> <out-dir> <globs>"
src=$1 out=$2
[ -d "$src" ] || die "no renders under $src"
IFS=, read -r -a globs <<<"$3"

mkdir -p "$out"
out=$(cd "$out" && pwd)
cd "$src"
count=0
while IFS= read -r path; do
  path=${path#./}
  for glob in "${globs[@]}"; do
    # Trim the spaces around each glob ("a.png, b.png").
    glob=${glob#"${glob%%[![:space:]]*}"}
    glob=${glob%"${glob##*[![:space:]]}"}
    # shellcheck disable=SC2254 # the glob is meant to match as a pattern
    case "$path" in
      $glob)
        mkdir -p "$out/$(dirname "$path")"
        cp "$path" "$out/$path"
        count=$((count + 1))
        break
        ;;
    esac
  done
done < <(find . -type f -name '*.png' | sort)

[ "$count" -gt 0 ] || die "no PNG under $src matches '$3'"
echo "ci-screens: $count PNGs matching '$3'"
