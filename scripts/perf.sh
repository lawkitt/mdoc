#!/usr/bin/env bash
# Usage: scripts/perf.sh
# Runs every #[ignore]d performance test serially. Results are only meaningful
# on an idle machine; report profile, machine, latency and peak memory
# (docs/development.md). Set MDOC_PERF_PREVIEW to a local PDF or DOCX to add
# the preview timing.
source "$(dirname "$0")/lib.sh"

failures=0
profile_args=(--profile test)
case "${MDOC_PERF_PROFILE:-debug}" in
  debug) ;;
  release) profile_args=(--release -j 1) ;;
  *) die "MDOC_PERF_PROFILE must be debug or release" ;;
esac

run() {
  echo "=== $1 :: $2"
  if ! cargo test --locked "${profile_args[@]}" -p "$1" "$2" -- --ignored --nocapture --test-threads=1; then
    failures=$((failures + 1))
  fi
}

run mdoc host_performance_matrix
run mdoc tabs_host_performance
run mdoc review_host_performance
run mdoc expanded_panel_projection_performance
run mdoc pii_annotation_geometry_matrix
run mdoc large_markdown_scroll_budget
[ -n "${MDOC_PERF_PREVIEW:-}" ] && run mdoc local_preview_performance
run mdoc-spell spelling_cache_performance
run mdoc-editor search_performance_matrix
run gpui-pdf long_document_scroll_budget
run mdoc-pii identity_policy_history_shares_indices_and_prunes_states
run mdoc-pii utf8_matcher_storage_matrix
run mdoc-pii metadata_history_stress_retains_shared_originals_and_prunes_reversible_deltas

[ "$failures" -eq 0 ] || die "$failures performance checks failed; all checks were attempted"
