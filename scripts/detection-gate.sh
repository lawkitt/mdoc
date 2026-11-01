#!/usr/bin/env bash
# Usage: scripts/detection-gate.sh (`just detection-gate`)
#
# The detection gate (ADR 0093), macOS only; the Mac's local CI runs it in
# local/macos after the mobile check. It reruns the pinned FP16 model, read
# from the app data and never downloaded, over every recognition fixture
# (`just recognize --check`): a stale snapshot, a fixture not as its status
# says, or a model run over its 5-minute budget fails. It runs when the
# change (HEAD^1 to HEAD: on a merge ref, the PR) touches what the model
# stage or the fixtures depend on: mdoc-detect, mdoc-pii, mdoc-import,
# mdoc-store, the fixtures, the model pins, Cargo.lock or the toolchain.
# MDOC_DETECTION_GATE=always or never overrides. `just test` already runs
# the fixtures on their snapshots on every OS.
source "$(dirname "$0")/lib.sh"

[ "$(uname -s)" = Darwin ] || die "the detection gate runs on macOS, the reference platform"

affected() {
  case "${MDOC_DETECTION_GATE:-}" in
    always) return 0 ;;
    never) return 1 ;;
  esac
  local names
  git rev-parse --verify --quiet HEAD^1 >/dev/null || return 0
  # The whole list first: grep -q stopping early would fail the pipe.
  names=$(git diff --no-renames --name-only HEAD^1 HEAD) || die "cannot list the change's files"
  grep -qE '^(crates/mdoc-(detect|pii|import|store)/|tests/fixtures/recognition/|resources/pseudonymization|Cargo\.lock$|rust-toolchain\.toml$|justfile$|scripts/detection-gate\.sh$)' <<<"$names"
}

if ! affected; then
  echo "detection-gate: no change to recognition or its dependencies; skipped"
  exit 0
fi
just recognize --check
