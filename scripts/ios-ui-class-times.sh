#!/usr/bin/env bash
# Usage: scripts/ios-ui-class-times.sh [--days N] [--from <folder>] [--dry-run]
# The nightly's refresh of apps/ios/ui-class-times.tsv, the UI shards'
# class times (ADR 0113): the green UI parts' times of the last N days (14),
# from their `ios-ui-times-*` artifacts (or the `.tsv` files in --from), as
# a new table of the classes under apps/ios/mdocUITests; a class measured
# by fewer than 3 parts keeps its row, so one slow run moves nothing (a
# class without a row takes even one measurement). When it would move
# a class to the other shard, or measures a class the table lacks, it
# commits the table as one commit on this checkout's HEAD (main) and
# force-pushes it to the branch nightly/ui-class-times, unless that branch
# already holds the same table on the same commit; otherwise it deletes
# that branch when it exists (its refresh is merged or out of date), and a
# change of times that moves no class is left out, as it changes no shard.
# It never pushes to main and opens no PR: the organization's
# Actions policy lets no workflow open one, so the hub opens it from the
# branch with its own token, and CI starts as on any PR. It says so, with
# the classes that moved, in a notice on the run and the job summary
# ($GITHUB_STEP_SUMMARY). --dry-run writes the table and prints the plan but
# commits nothing. GH_REPO and GH_TOKEN read the artifacts (actions: read);
# the push needs contents: write. Bash 3.2 compatible.
source "$(dirname "$0")/lib.sh"

days=14 from='' dry=''
while [ $# -gt 0 ]; do
  case "$1" in
    --days)
      [[ ${2:-} =~ ^[1-9][0-9]*$ ]] || die "--days takes a whole number of days"
      days=$2
      shift
      ;;
    --from)
      [ -d "${2:-}" ] || die "--from takes a folder"
      from=$2
      shift
      ;;
    --dry-run) dry=1 ;;
    *) die "usage: scripts/ios-ui-class-times.sh [--days N] [--from <folder>] [--dry-run]" ;;
  esac
  shift
done

table=apps/ios/ui-class-times.tsv
branch=nightly/ui-class-times
py=$(command -v python3 || command -v python) || die "no python3"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

if [ -z "$from" ]; then
  from=$tmp/parts
  "$py" scripts/ios-ui-class-times.py fetch --days "$days" "$from"
fi
"$py" scripts/ios-ui-class-times.py table "$from" --days "$days" >"$tmp/all.tsv"

# Only the classes there now (a gone class's old times leave the table),
# each with its new row when at least $min_parts parts measured it, else
# its old row, else its new row.
min_parts=3
bash scripts/ios-ui-shards.sh --classes >"$tmp/classes"
# shellcheck disable=SC2016 # awk fields, not shell ones
awk -F '\t' -v min="$min_parts" -v rows="$tmp/rows" '
  FILENAME == ARGV[1] { keep[$1] = 1; next }
  FILENAME == ARGV[2] { sub(/\r$/, ""); if (!/^#/ && NF) old[$1] = $0; next }
  /^#/ { print; next }
  ($1 in keep) { new[$1] = $0; parts[$1] = $3 }
  END {
    for (c in keep) {
      if ((c in new) && (parts[c] >= min || !(c in old))) print new[c] >rows
      else if (c in old) print old[c] >rows
    }
  }' "$tmp/classes" "$table" "$tmp/all.tsv" >"$tmp/new.tsv"
[ ! -f "$tmp/rows" ] || LC_ALL=C sort "$tmp/rows" >>"$tmp/new.tsv"

before=$(bash scripts/ios-ui-shards.sh 1)
after=$(MDOC_UI_CLASS_TIMES="$tmp/new.tsv" bash scripts/ios-ui-shards.sh 1)
# shellcheck disable=SC2016 # awk fields, not shell ones
measured=$(awk -F '\t' '
  FILENAME == ARGV[1] { if (!/^#/) old[$1] = 1; next }
  !/^#/ && !($1 in old) { print $1 }' "$table" "$tmp/new.tsv")
if [ "$before" = "$after" ] && [ -z "$measured" ]; then
  echo "ui-class-times: no class changes shard and every measured class is in $table; nothing to do"
  # An earlier refresh still waiting on the branch is out of date (or
  # merged): deleting the branch also closes a PR opened from it.
  if [ -z "$dry" ] && git ls-remote --exit-code -q origin "refs/heads/$branch" >/dev/null 2>&1; then
    git push -q origin --delete "$branch"
    echo "::notice title=UI class times::Deleted $branch: its refresh is merged or no longer moves a class."
  fi
  exit 0
fi

bash scripts/ios-ui-shards.sh --plan >"$tmp/old-plan"
cp "$tmp/new.tsv" "$table"
bash scripts/ios-ui-shards.sh --plan >"$tmp/new-plan"
cat "$tmp/new-plan"
[ -z "$dry" ] || exit 0

# The classes that changed shard, as `Name (1 → 2)`.
# shellcheck disable=SC2016 # awk fields, not shell ones
moved=$(awk '
  /^shard / { next }
  FILENAME == ARGV[1] { was[$2] = $1; next }
  ($2 in was) && was[$2] != $1 { printf "%s%s (%s → %s)", (n++ ? ", " : ""), $2, was[$2], $1 }
' "$tmp/old-plan" "$tmp/new-plan")
parts=$(find "$from" -name '*.tsv' | wc -l | tr -d ' ')
summary="A refresh of $table waits on the branch $branch: open a PR from it. Moved: ${moved:-none}.${measured:+ Newly measured: $(printf '%s\n' "$measured" | paste -sd ' ' -).}"
message="iOS UI shards: refreshed class times (nightly)

The nightly's refresh of $table (ADR 0113, scripts/ios-ui-class-times.sh)
from $parts green UI parts of the last $days days. Moved: ${moved:-none}.
${measured:+Newly measured: $(printf '%s\n' "$measured" | paste -sd ' ' -).
}
Before:
$(grep '^shard ' "$tmp/old-plan")

After (scripts/ios-ui-shards.sh --plan):
$(cat "$tmp/new-plan")

Run: ${RUN_URL:-unknown}"

git checkout -q -B "$branch"
git add "$table"
git -c user.name='github-actions[bot]' -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
  commit -q -m "$message"
# The same table on the same main commit: nothing new to push.
if git fetch -q origin "refs/heads/$branch" 2>/dev/null &&
  [ "$(git rev-parse 'FETCH_HEAD^{tree}')" = "$(git rev-parse 'HEAD^{tree}')" ] &&
  [ "$(git rev-parse FETCH_HEAD~1)" = "$(git rev-parse HEAD~1)" ]; then
  echo "ui-class-times: $branch already holds this refresh"
else
  git push -q --force origin "HEAD:refs/heads/$branch"
  echo "ui-class-times: pushed $branch"
fi
echo "::notice title=UI class times::$summary"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### UI class times"
    echo
    echo "$summary"
    echo
    echo '```'
    cat "$tmp/new-plan"
    echo '```'
  } >>"$GITHUB_STEP_SUMMARY"
fi
