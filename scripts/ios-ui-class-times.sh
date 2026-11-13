#!/usr/bin/env bash
# Usage: scripts/ios-ui-class-times.sh [--days N] [--from <folder>] [--dry-run]
# The nightly's refresh of apps/ios/ui-class-times.tsv, the UI shards'
# class times (ADR 0113): the green UI parts' times of the last N days (14),
# from their `ios-ui-times-*` artifacts (or the `.tsv` files in --from), as
# a new table of the classes under apps/ios/mdocUITests. When it would move
# a class to the other shard, or measures a class the table lacks, it
# commits the table on the branch nightly/ui-class-times, on top of this
# checkout's HEAD (main), and opens one PR with it, labelled
# nightly-refresh, or updates the open one; otherwise it changes nothing. It
# never pushes to main: the hub enqueues the PR. --dry-run writes the table
# and prints the plan but commits nothing. GH_REPO and GH_TOKEN reach
# GitHub (actions: read; contents and pull-requests: write, issues: write
# for the label). Bash 3.2 compatible.
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
label=nightly-refresh
py=$(command -v python3 || command -v python) || die "no python3"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

if [ -z "$from" ]; then
  from=$tmp/parts
  "$py" scripts/ios-ui-class-times.py fetch --days "$days" "$from"
fi
"$py" scripts/ios-ui-class-times.py table "$from" --days "$days" >"$tmp/all.tsv"

# Only the classes there now: a gone class's old times leave the table.
classes=$(bash scripts/ios-ui-shards.sh --classes)
printf '%s\n' "$classes" >"$tmp/classes"
# shellcheck disable=SC2016 # awk fields, not shell ones
awk -F '\t' 'NR == FNR { keep[$1] = 1; next } /^#/ || ($1 in keep)' "$tmp/classes" "$tmp/all.tsv" >"$tmp/new.tsv"

before=$(bash scripts/ios-ui-shards.sh 1)
after=$(MDOC_UI_CLASS_TIMES="$tmp/new.tsv" bash scripts/ios-ui-shards.sh 1)
# shellcheck disable=SC2016 # awk fields, not shell ones
measured=$(awk -F '\t' '
  FILENAME == ARGV[1] { if (!/^#/) old[$1] = 1; next }
  !/^#/ && !($1 in old) { print $1 }' "$table" "$tmp/new.tsv")
if [ "$before" = "$after" ] && [ -z "$measured" ]; then
  echo "ui-class-times: no class changes shard and every measured class is in $table; nothing to do"
  exit 0
fi

old_plan=$(bash scripts/ios-ui-shards.sh --plan | grep '^shard ')
cp "$tmp/new.tsv" "$table"
plan=$(bash scripts/ios-ui-shards.sh --plan)
echo "$plan"
[ -z "$dry" ] || exit 0

parts=$(find "$from" -name '*.tsv' | wc -l | tr -d ' ')
body="The nightly's refresh of \`$table\` (ADR 0113, \`scripts/ios-ui-class-times.sh\`) from $parts green UI parts of the last $days days.

Before:

\`\`\`
$old_plan
\`\`\`

After (\`scripts/ios-ui-shards.sh --plan\`):

\`\`\`
$plan
\`\`\`
${measured:+
Newly measured: $(echo "$measured" | tr '\n' ' ' | sed 's/ $//').
}
The workflow's token opened or pushed this, which starts no workflow: close and reopen the PR to start \`lint (ubuntu)\` and the fast tier, then enqueue it. Run: ${RUN_URL:-unknown}."

git checkout -q -B "$branch"
git add "$table"
git -c user.name='github-actions[bot]' -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
  commit -q -m "iOS UI shards: refreshed class times (nightly)"
git push -q --force origin "HEAD:refs/heads/$branch"

pr=$(gh pr list --head "$branch" --base main --state open --json number --jq '.[0].number // empty')
if [ -n "$pr" ]; then
  gh pr edit "$pr" --body "$body" >/dev/null
  echo "ui-class-times: updated #$pr"
else
  gh label create "$label" --color 0E8A16 --force \
    --description "Refreshed data the nightly opened as a PR; the hub enqueues it" >/dev/null
  gh pr create --base main --head "$branch" --label "$label" \
    --title "iOS UI shards: refreshed class times (nightly)" --body "$body"
fi
