#!/usr/bin/env bash
# Usage: scripts/ci-areas.sh [<from> <to>]
#        scripts/ci-areas.sh --github
#        scripts/ci-areas.sh --queue
#
# The areas a change touches, by scripts/ci-areas.txt (ADR 0091): one line
# of core, desktop, recognition, ios, android and ci, in that order; empty
# for docs only.
# A core change lists every area. <from> and <to> default to HEAD^1 and
# HEAD: on a merge ref, what the PR brings onto its base.
#
# --github is ci.yml's areas job: from GitHub Actions' environment (EVENT,
# BEFORE, RELEASE_CALL) it writes `areas=<list>` and `desktop=true|false`
# for $GITHUB_OUTPUT on stdout. A PR's merge commit is diffed against its
# base, a push against the commit before it; a manual run, a release call or
# a push whose earlier commit is unknown lists every area. On a PR it also
# writes `detection-head=true|false`: whether the head runs the detection
# gate (head_detection in lib.sh, ADR 0100).
#
# --queue is queue.yml's areas job (ADR 0095): what a queue commit brings
# onto main, from main's merge base (origin/main, fetched), the same diff
# merge-gate reads. It writes `areas=<list>`, `jobs=<list>` (needed_jobs)
# and `<job>=true|false` for macos, ios, detection and scripts, the hosted
# jobs queue.yml starts. Windows and Android run on the PC.
source "$(dirname "$0")/lib.sh"

every='core desktop recognition ios android ci'

github() {
  local areas=$every head=''
  case "${EVENT:-}" in
    pull_request)
      areas=$(diff_areas HEAD^1 HEAD) || areas=$every
      # Not when the diff fails: merge-gate takes a skipped head run as not
      # needed, and the queue still runs the gate.
      head=false
      if git diff --no-renames --name-only -z HEAD^1 HEAD | head_detection "$(cat scripts/ci-areas.txt)"; then
        head=true
      fi
      ;;
    push)
      if [ "${RELEASE_CALL:-false}" != true ] && [[ ${BEFORE:-} =~ ^[0-9a-f]{40}$ ]] &&
        [[ $BEFORE != 0000000000000000000000000000000000000000 ]]; then
        git cat-file -e "$BEFORE^{commit}" 2>/dev/null ||
          git fetch --quiet --no-tags --depth=1 origin "$BEFORE" || true
        areas=$(diff_areas "$BEFORE" HEAD 2>/dev/null) || areas=$every
      fi
      ;;
  esac
  echo "areas=$areas"
  # Hosted clippy and test also run the recognition fixtures: on main they
  # are the hosted check of a fixture change (the Mac's runner tests it too).
  if affects desktop "$areas" || affects recognition "$areas"; then echo desktop=true; else echo desktop=false; fi
  [ -z "$head" ] || echo "detection-head=$head"
}

queue() {
  local base areas=$every jobs job
  # merge-gate counts every area from 300 files (GitHub's compare lists
  # at most 300), so the queue does too.
  if base=$(git merge-base origin/main HEAD 2>/dev/null) &&
    [ "$(git diff --no-renames --name-only "$base" HEAD | wc -l)" -lt 300 ]; then
    areas=$(diff_areas "$base" HEAD) || areas=$every
  fi
  jobs=$(needed_jobs "$areas")
  echo "areas=$areas"
  echo "jobs=$jobs"
  for job in macos ios detection scripts; do
    if affects "$job" "$jobs"; then echo "$job=true"; else echo "$job=false"; fi
  done
}

case "${1:-}" in
  --github) github ;;
  --queue) queue ;;
  '') diff_areas HEAD^1 HEAD ;;
  *)
    [ $# -eq 2 ] || die "usage: scripts/ci-areas.sh [<from> <to>] | --github | --queue"
    diff_areas "$1" "$2"
    ;;
esac
