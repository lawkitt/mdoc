#!/usr/bin/env bash
# Usage: scripts/ios/ios-ci-parts.sh all | touched | <part>[,<part>...]
# The parts of ci-ios.yml a run tests, as its matrix's JSON list
# (`["unit","ui 2"]`), in order. `all`: the unit part and every UI shard
# (scripts/ios/ios-ui-shards.sh), as the queue and the nightly run them.
# `touched`, a branch dispatch's default: what HEAD changed since its merge
# base with origin/main (MDOC_MAIN_REF replaces it), the UI shards holding
# the changed UI test classes (ios-ui-shards.sh --touched), and the unit
# part when the change is under apps/ios/mdocTests or touches no UI shard.
# A deleted Swift file there counts by its old copy: a test class touches
# no shard, a helper (no class, such as Waits.swift) every one; a moved
# file counts as deleted at its old path and added at its new one.
# A list (`unit,ui 2`; `ui2` too) names the parts. One line only.
# Bash 3.2 compatible.
source "$(dirname "$0")/../lib.sh"

usage="usage: scripts/ios/ios-ci-parts.sh all | touched | <part>[,<part>...]"
if [ "$#" -ne 1 ] || [ -z "$1" ]; then die "$usage"; fi
case "$1" in *$'\n'* | *$'\r'*) die "parts is one line, not: $1" ;; esac

count=$(bash scripts/ios/ios-ui-shards.sh --plan | grep -c '^shard ') || die "could not read the UI shards"
unit='' ui=''

case "$1" in
  all)
    unit=1
    ui=$(seq 1 "$count")
    ;;
  touched)
    main=${MDOC_MAIN_REF:-origin/main}
    base=$(git merge-base "$main" HEAD) || die "no merge base of HEAD and $main"
    scratch=$(mktemp -d)
    trap 'rm -rf "$scratch"' EXIT
    # NUL-separated, so no path is quoted; no rename detection, so git
    # reads no file contents for the list (CI's clone has none).
    git diff --name-only -z --no-renames --diff-filter=d "$base" HEAD >"$scratch/kept"
    git diff --name-only -z --no-renames --diff-filter=D "$base" HEAD >"$scratch/deleted"
    ui=$(xargs -0 bash scripts/ios/ios-ui-shards.sh --touched <"$scratch/kept")
    while IFS= read -r -d '' path; do
      case "$path" in apps/ios/mdocUITests/*.swift) ;; *) continue ;; esac
      git show "$base:$path" >"$scratch/old.swift" || die "could not read $path at $base"
      [ -n "$(bash scripts/ios/ios-ui-shards.sh --declared "$scratch/old.swift")" ] ||
        ui=$(printf '%s\n' "$ui" && seq 1 "$count")
    done <"$scratch/deleted"
    ui=$(printf '%s\n' "$ui" | awk 'NF' | LC_ALL=C sort -nu)
    # Not grep -q: its early exit fails the pipe (pipefail).
    if [ -z "$ui" ] || cat "$scratch/kept" "$scratch/deleted" | tr '\0' '\n' | grep '^apps/ios/mdocTests/' >/dev/null; then unit=1; fi
    ;;
  *)
    IFS=, read -ra asked <<<"$1"
    for part in "${asked[@]}"; do
      # Spaces around a comma, or none in `ui2`.
      part=$(tr -d ' ' <<<"$part")
      case "$part" in
        unit) unit=1 ;;
        ui[1-9])
          [ "${part#ui}" -le "$count" ] || die "no part ui ${part#ui}: there are $count UI shards"
          ui=$(printf '%s\n%s\n' "$ui" "${part#ui}")
          ;;
        *) die "no part '$part': the parts are unit and ui 1 to ui $count, or all or touched" ;;
      esac
    done
    ;;
esac

{
  [ -z "$unit" ] || echo unit
  printf '%s\n' "$ui" | awk 'NF' | LC_ALL=C sort -nu | sed 's/^/ui /'
} | awk 'BEGIN { ORS = "" } { printf "%s\"%s\"", (NR > 1 ? "," : "["), $0 } END { print (NR ? "]" : "[]") "\n" }'
