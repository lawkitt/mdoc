#!/usr/bin/env bash
# Usage: scripts/deny-ci.sh
#
# CI's cargo-deny step (ADR 0095): `just deny-policy` (licences, bans,
# sources) always, and `just deny-advisories` only when the change touches
# Cargo.lock or deny.toml, so an advisory published today turns red the PR
# that brings the crate in and the nightly run (which opens an issue), never
# every PR. From GitHub Actions' environment: EVENT (github.event_name),
# BASE (the commit the change is compared with: a PR's base, a queue
# commit's base, a push's previous commit) and RELEASE_CALL. A release
# call, a manual or scheduled run, or a base this checkout cannot fetch
# runs both. Bash 3.2 compatible.
source "$(dirname "$0")/lib.sh"

advisories=true
case "${EVENT:-}" in
  pull_request | merge_group | push)
    base=${BASE:-}
    if [ "${RELEASE_CALL:-false}" != true ] && [[ $base =~ ^[0-9a-f]{40}$ ]] &&
      [[ $base != 0000000000000000000000000000000000000000 ]]; then
      git cat-file -e "$base^{commit}" 2>/dev/null ||
        git fetch --quiet --no-tags --depth=1 origin "$base" 2>/dev/null || true
      if git cat-file -e "$base^{commit}" 2>/dev/null &&
        git diff --quiet "$base" HEAD -- Cargo.lock deny.toml; then
        advisories=false
      fi
    fi
    ;;
esac

just deny-policy
if $advisories; then
  just deny-advisories
else
  echo "deny-ci: Cargo.lock and deny.toml unchanged; advisories run nightly"
fi
