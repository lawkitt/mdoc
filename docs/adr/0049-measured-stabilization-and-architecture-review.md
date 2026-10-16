# Measured stabilization and architectural review

Status: accepted and focused implementation completed, 2026-10-16. The user agreed
to the focused scope and performance tiers, then to the architectural ownership,
conditional layout investigation and quit-failure recommendations in the
[audit](../evidence/stabilization/2026-10-16/README.md).
[Implementation and measurements](../evidence/stabilization/2026-10-16/results.md)
record verification and remaining performance limits. The broader architectural
projects below remain roadmap work.

## Problem

Feature growth since the last cleanup has added spelling, encrypted cases,
cross-document mapping updates, AI-response restoration, before/after views,
updates and a new workspace shell. The October 16 audit found overlapping
refresh effects, repeated computation and a quit path that discards case-save
errors. Correctness tests pass, but the ordinary host performance matrix fails
and unchanged document redraw is expensive in the debug/headless measurements.

## Decision

- Focus the immediate stabilization scope on the review's F1–F6: persistence
  failures, diagnostic/layout work, spelling cache and scheduling, panel
  preparation and reproducible performance coverage.
- Preserve current features and interaction design. Judge refactoring by clearer
  ownership, explicit effects, correctness and measured cost; file length alone
  does not justify extraction.
- Ordinary legal documents are the hard interactive target. Large/dense inputs,
  including 2 MiB / 20,000 mentions, form a measured stress tier with bounded
  work and memory; they do not yet carry the same latency commitment.
- Choose numerical budgets after representative release/native baselines at
  recorded effective viewports. Debug/headless timings expose costs but do not
  establish native input-to-display latency or release regressions.
- Track case coordination and document lifecycle as separate, incremental
  architectural roadmap projects. Consolidate their domain transitions into
  explicit owners in the existing app/modules; GPUI keeps presentation, prompts
  and scheduling. Preserve existing editor/history, import and preview owners.
  Define each extraction's exact migration boundary before implementing it.
- Keep retained editor layout as a profiling-gated investigation after local
  stabilization. Commit to that migration only if remaining release-build cost
  justifies its complexity and geometry/editing risks.
- During quit, wait asynchronously for case saves. A failure keeps the window
  and mappings available and offers **Retry / Cancel / Quit without saving
  mappings**. Retain mappings until retry succeeds or the user explicitly quits;
  do not silently discard the failure or block the UI thread on persistence.

## Invariants and consequences

ADRs 0010, 0011, 0021 and 0029 continue to govern applicable behavior and test
preservation. Keep source/history ownership, occurrence provenance, stable
aliases, dirty state, cancellation/revision guards, worker-owned admission and
preview resource lifetimes. Preserve encrypted case storage and stale-write
protection. Case-wide synchronization still produces edits in each member's
editor history; this does not introduce another undo owner or redesign undo.

Use deterministic checks for routine regressions and controlled timing/resource
measurements for performance acceptance. Do not raise failing ceilings merely
to make the current tree pass. No feature redesign, generalized service framework
or text-storage replacement follows automatically from this decision.

## Delivery boundary

The immediate pass addresses the concrete audit findings. The two broader
ownership projects stay on the roadmap; completing them is not required merely
to fix the current defects. Retained layout remains conditional on profiling.

Verification must cover failed/retried/cancelled quit, pending and late writes,
stale/deleted case protection, repeated-line cache reuse, bounded spelling work,
and correct derived-data invalidation through edits, undo/redo and case updates.
Run the repository gate and affected performance checks with public fixtures and
recorded effective viewports. Measure the release/native baseline before setting
numerical targets; report unavailable native/platform checks explicitly.

## Pre-merge review follow-up (2026-10-16)

A pre-merge review of the stabilization branch found defects that block merge.
They are fixed on this branch, one commit per finding:

- An abandoned quit must clear all case-quit state. Every path that ends
  `finishing` (a session-write failure, a cancelled close or a cancelled
  mapping prompt) must also clear the pending quit window, error and prompt.
  Otherwise a later ordinary case-save failure is treated as a quit failure and
  blocks every later case save without a prompt. This fix includes a
  regression test.
- A failed write for a case that was removed during the write still clears its
  close-after state and continues writing the other queued cases.
- `MDOC_PERF_STRESS` enables the stress tier only when it is set to `1`, as
  documented.

The review also suspected that spelling could stay marked as running if its
worker ended without returning. It cannot: release builds abort on panic, and
the worker handle is replaced only when no check is running. Keep that
invariant if either changes.

Known gaps, deferred to a follow-up after merge and closed there:

- Derived review data (panel projection, before/after view, spelling exclusions)
  was invalidated by manual `pii.version` bumps through `review_changed`.
  Closed: `Review::revision()` is advanced by every `&mut` method of `Review`,
  and the derived data is keyed on it. Revisions come from one process-wide
  counter, so a review replaced by a new one never repeats an earlier value.
  `review_changed` now only schedules spellcheck and the case save; paint-only
  syncs (theme change, panel open) still schedule nothing.
- Case write completion during quit scheduled the final quit check twice.
  Closed: `case_write_failed` no longer resumes the quit, and each caller
  resumes it once.
- The quit check scanned the list of checked tabs once per tab. Closed:
  `quit_checked` is a map keyed by tab id.

Before merge: run `cargo test` and the debug `just perf`. Repeat the release
measurements only if a fix touches a measured hot path.
