#!/usr/bin/env bash
# Usage: scripts/nightly-issue.sh <job>=<result>...
#
# The nightly run's report (ADR 0095): one open issue titled "Nightly CI is
# red" while any job of the run did not succeed. A red night opens it, or
# comments on the one already open, naming the failed jobs and linking the
# run; a green night closes it with a comment. <result> is a job's
# `needs.<job>.result` (success, failure, cancelled or skipped); skipped
# counts as red too, since every nightly job should run. RUN_URL links the
# run; GH_REPO and GH_TOKEN (issues: write) reach GitHub. Bash 3.2
# compatible.
source "$(dirname "$0")/lib.sh"

title="Nightly CI is red"
[ $# -gt 0 ] || die "usage: scripts/nightly-issue.sh <job>=<result>..."

failed='' passed=''
for pair in "$@"; do
  job=${pair%%=*} result=${pair#*=}
  if [ "$result" = success ]; then passed+=" $job"; else failed+=" $job ($result)"; fi
done
failed=${failed# } passed=${passed# }

issue=$(gh issue list --state open --search "\"$title\" in:title" --json number,title \
  --jq ".[] | select(.title == \"$title\") | .number" | head -1)

if [ -z "$failed" ]; then
  if [ -n "$issue" ]; then
    gh issue close "$issue" --comment "Green again: ${RUN_URL:-the latest nightly run} (${passed})."
    echo "nightly: green; closed #$issue"
  else
    echo "nightly: green"
  fi
  exit 0
fi

body="Failed: ${failed}. Passed: ${passed:-none}. Run: ${RUN_URL:-unknown}.

A failing job's log names the test; fix it, or quarantine a flaky test (\`.config/quarantine.txt\`). The next green night closes this issue."
if [ -n "$issue" ]; then
  gh issue comment "$issue" --body "$body"
  echo "nightly: red; commented on #$issue"
else
  gh issue create --title "$title" --body "$body"
  echo "nightly: red; opened an issue"
fi
