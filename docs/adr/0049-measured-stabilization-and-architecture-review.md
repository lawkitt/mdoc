# Measured stabilization and architectural review

Status: accepted, 2026-10-16; focused implementation in progress. The user agreed
to the focused scope and performance tiers, then to the architectural ownership,
conditional layout investigation and quit-failure recommendations in the
[review](../design/maintainability-review.md).

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
