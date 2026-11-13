#!/usr/bin/env bash
# Usage: scripts/ci-targets-issue.sh [scripts/ci-stats.sh arguments]
#
# The nightly's check of ADR 0113's CI targets: runs scripts/ci-stats.sh with
# the arguments given (the nightly: --days 7 --nights 3) and keeps one issue,
# "CI targets missed":
#   - a target missed in every window measured (ci-stats exit 3: the last 3
#     nights) opens it, or comments on the one already open;
#   - a target missed only lately (exit 2) comments on the open one, if any;
#   - every target held (exit 0) closes the open one with a comment.
# A ci-stats error (exit 1) prints its output and fails, so the nightly's own
# report names this job. Not nightly-issue.sh: that one opens its issue on
# the first red night and reads job results, not a measurement. CI_STATS
# names the script to run (tests); RUN_URL links the run; GH_REPO and
# GH_TOKEN (issues: write, actions and statuses: read) reach GitHub. Bash 3.2
# compatible; scripts/test-ci-stats.sh checks it against fakes.
source "$(dirname "$0")/lib.sh"

title="CI targets missed"
stats=${CI_STATS:-$(dirname "$0")/ci-stats.sh}

out=$(bash "$stats" "$@") && rc=0 || rc=$?
printf '%s\n' "$out"
case "$rc" in
  0 | 2 | 3) ;;
  *) die "ci-stats.sh failed (exit $rc)" ;;
esac

issue=$(gh issue list --state open --search "\"$title\" in:title" --json number,title \
  --jq ".[] | select(.title == \"$title\") | .number" | head -1) || die "could not list the issues"

if [ "$rc" -eq 0 ]; then
  if [ -n "$issue" ]; then
    gh issue close "$issue" --comment "Every target held: ${RUN_URL:-the latest nightly run}." || die "could not close #$issue"
    echo "ci-targets: held; closed #$issue"
  else
    echo "ci-targets: held"
  fi
  exit 0
fi

if [ "$rc" -eq 3 ]; then
  lead="A target was missed in every window measured: on each of the last nights (ADR 0113)."
else
  lead="A target was missed in the latest window (ADR 0113)."
fi
body="$lead Run: ${RUN_URL:-unknown}.

\`\`\`text
$out
\`\`\`

\`scripts/ci-stats.sh\`'s header says what each line counts. The next night that meets every target closes this issue."

if [ -n "$issue" ]; then
  gh issue comment "$issue" --body "$body" || die "could not comment on #$issue"
  echo "ci-targets: missed; commented on #$issue"
elif [ "$rc" -eq 3 ]; then
  gh issue create --title "$title" --body "$body" || die "could not open the issue"
  echo "ci-targets: missed every night; opened an issue"
else
  echo "ci-targets: missed lately, not yet every night; no issue"
fi
