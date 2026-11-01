#!/usr/bin/env bash
# Usage: scripts/ci-areas.sh [<from> <to>]
#        scripts/ci-areas.sh --github
#
# The areas a change touches, by scripts/ci-areas.txt (ADR 0091): one line
# of core, desktop, ios and android, in that order; empty for docs only.
# A core change lists every area. <from> and <to> default to HEAD^1 and
# HEAD: on a merge ref, what the PR brings onto its base.
#
# --github is ci.yml's areas job: from GitHub Actions' environment (EVENT,
# BEFORE, RELEASE_CALL) it writes `areas=<list>` and `desktop=true|false`
# for $GITHUB_OUTPUT on stdout. A PR's merge commit is diffed against its
# base, a push against the commit before it; a manual run, a release call or
# a push whose earlier commit is unknown lists every area.
source "$(dirname "$0")/lib.sh"

every='core desktop ios android'

github() {
  local areas=$every
  case "${EVENT:-}" in
    pull_request)
      areas=$(diff_areas HEAD^1 HEAD) || areas=$every
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
  if affects desktop "$areas"; then echo desktop=true; else echo desktop=false; fi
}

case "${1:-}" in
  --github) github ;;
  '') diff_areas HEAD^1 HEAD ;;
  *)
    [ $# -eq 2 ] || die "usage: scripts/ci-areas.sh [<from> <to>] | --github"
    diff_areas "$1" "$2"
    ;;
esac
