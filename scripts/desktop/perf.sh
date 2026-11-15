#!/usr/bin/env bash
# Usage: scripts/desktop/perf.sh
# Runs every #[ignore]d performance test serially (`just perf`), outside the
# routine gate, attempting every check before it fails. An ordinary legal
# document (about 30 pages, 135 KB of Markdown) is the hard interactive
# target, judged on a release build on the M4 reference Mac (p95 frame within
# 16.7 ms); these debug ceilings are regression guards. Results are only
# meaningful on an idle machine: use a fixed viewport, representative legal or
# OCR Markdown and real scroll and search positions, and report profile,
# machine, latency, peak memory and lifecycle results. Logs give the editor's
# real width; outer window sizes alone cannot be compared, nor different
# workloads, nor timings from different CPU models. Reproduce a cost
# before optimizing it. Debug and headless timings expose costs but establish
# neither native latency nor a release regression; never raise a failing
# ceiling to make the tree pass.
#
#   MDOC_PERF_PROFILE=release  the release profile, serial builds, the same
#                              ceilings
#   MDOC_PERF_PREVIEW=<file>   add the preview timing for a local PDF or DOCX
#   MDOC_PERF_WORKLOADS=a,b    only these host-matrix workloads (`contract`, a
#                              synthetic ~30-page agreement; `legal_opinion`,
#                              tests/fixtures/docx-preview/legal-opinion.docx;
#                              `tables`; ...)
#   MD_PERF_FILE=<name>        one DOCX under tests/fixtures/docx-preview for
#                              the DOCX scroll workload (default: coverage.docx
#                              and legal-opinion.docx)
#   PDF_PERF_DIR=<dir>         a local PDF corpus (default: reference.pdf and a
#                              generated 64-page PDF)
#   MDOC_PERF_STRESS=1         the review matrix at the stress tier (2 MiB,
#                              20,000 mentions): bounded work and memory, no
#                              latency commitment; run it in release mode
#
# review_host_performance's `plain_layout` phase draws the same text without
# review state at the same width, so each review phase's excess over it is the
# PII overhead. Headless shaping is not Core Text: confirm shaping-heavy
# changes natively with tools/native-layout (native input, then an
# Animation Hitches trace summarized). Scrub a private DOCX into a layout fixture with
# tests/fixtures/docx-preview/scrub.py; never commit the original.
source "$(dirname "$0")/../lib.sh"

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
run mdoc-editor-core search_performance_matrix
run gpui-pdf long_document_scroll_budget
run mdoc-pii identity_policy_history_shares_indices_and_prunes_states
run mdoc-pii utf8_matcher_storage_matrix
run mdoc-pii metadata_history_stress_retains_shared_originals_and_prunes_reversible_deltas

[ "$failures" -eq 0 ] || die "$failures performance checks failed; all checks were attempted"
