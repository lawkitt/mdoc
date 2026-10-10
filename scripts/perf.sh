#!/usr/bin/env bash
# Usage: scripts/perf.sh
# Runs every #[ignore]d performance test serially. Results are only meaningful
# on an idle machine; report profile, machine, latency and peak memory
# (docs/development.md). Set MDOC_PERF_PREVIEW to a local PDF or DOCX to add
# the preview timing.
source "$(dirname "$0")/lib.sh"

run() {
  echo "=== $1 :: $2"
  cargo test -p "$1" "$2" -- --ignored --nocapture --test-threads=1
}

run mdoc host_performance_matrix
run mdoc tabs_host_performance
run mdoc pii_annotation_geometry_matrix
run mdoc large_markdown_scroll_budget
[ -n "${MDOC_PERF_PREVIEW:-}" ] && run mdoc local_preview_performance
run mdoc-editor search_performance_matrix
run gpui-pdf long_document_scroll_budget
run mdoc-pii identity_policy_history_shares_indices_and_prunes_states
run mdoc-pii utf8_matcher_storage_matrix
run mdoc-pii metadata_history_stress_retains_shared_originals_and_prunes_reversible_deltas
