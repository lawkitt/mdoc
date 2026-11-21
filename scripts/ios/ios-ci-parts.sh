#!/usr/bin/env bash
# Usage: scripts/ios/ios-ci-parts.sh all | touched | <part>[,<part>...]
# The parts of ci-ios.yml a run tests, as its matrix's JSON list
# (`["unit","ui 2"]`), in order. `all`: the unit part and every UI shard
# (scripts/ios/ios-ui-shards.sh), as the queue and the nightly run them.
# `touched`, a branch dispatch's default: what HEAD changed since its merge
# base with origin/main (MDOC_MAIN_REF replaces it), the UI shards holding
# the changed UI test classes (ios-ui-shards.sh --touched), and the unit
# part when the change is under apps/ios/mdocTests or touches no UI shard.
# A list (`unit,ui 2`; `ui2` too) names the parts. Bash 3.2 compatible.
source "$(dirname "$0")/../lib.sh"

usage="usage: scripts/ios/ios-ci-parts.sh all | touched | <part>[,<part>...]"
if [ "$#" -ne 1 ] || [ -z "$1" ]; then die "$usage"; fi

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
    # No rename detection: a moved file counts at its new path, and git
    # needs no file contents (CI's clone has none).
    files=$(git diff --name-only --no-renames "$base" HEAD)
    # shellcheck disable=SC2086 # split on lines, on purpose
    ui=$(IFS=$'\n' && bash scripts/ios/ios-ui-shards.sh --touched $files)
    if [ -z "$ui" ] || grep -q '^apps/ios/mdocTests/' <<<"$files"; then unit=1; fi
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
