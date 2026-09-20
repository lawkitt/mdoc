# Maintainability and performance refactoring decisions

Design interview, 2026-09-20. Both rounds and final shared understanding confirmed.
User authorized the audit and implementation.

## Confirmed: round 1

1. Inspect the app and bundled crates, then select two or three concrete
   hotspots. Internal interfaces may change across both. Prefer clearer
   ownership and predictable performance over a broad restructuring.
2. Preserve current user-facing behavior and Markdown fidelity. Remove dead
   code and redundant internal machinery where justified; feature removal
   requires a separate decision.
3. Establish representative performance baselines before refactoring, prevent
   regressions, and fix measured bottlenecks that intersect the selected work.
   Cover typing, scrolling, search, document opening, and memory after repeated
   document switches. Set numerical budgets after measurement.

Rationale: feature growth justifies an audit, not an automatic rewrite.
Performance claims need measurements of the affected interactions.

## Inspection informing round 2

- Workspace owns interleaved search, import, OCR and preview lifecycles,
  including task handles and stale-result guards (`src/main.rs`).
- Editor layout caches already exist; their invalidation rules deserve review
  (`crates/mdoc-editor/src/lib.rs`, `element.rs`). File size alone does not
  justify extraction.
- Search and PDF performance harnesses exist but are ignored by the default
  test gate. The search report excludes worker contention and painting from
  its measurements (`docs/markdown-search-performance.md`).
- Existing feature modules and targeted optimizations mean this is a review
  of ownership and performance contracts, not a claim that the project has
  never been refactored.

## Confirmed: round 2

4. Prioritize three areas:
   - Document work: clarify ownership of import, OCR, preview replacement,
     cancellation, and stale results.
   - Search: keep query scheduling, index reuse, and result publication
     together; measure rapid typing and highlight rendering.
   - Editor layout: make cache invalidation rules explicit and testable.
     Extract modules only where doing so simplifies ownership.
   Start with document/search ownership; layout changes depend on audit
   evidence rather than file size.
5. Put cheap, deterministic regression checks in the normal gate, including
   stale-result rejection and allocation-preserving search navigation. Run
   repeatable timing and memory benchmarks separately on a stable machine
   before merging changes to affected paths.
6. Cover ordinary, large, and pathological documents: long paragraphs,
   dense tables, many search matches, images, and PDF/DOCX previews. Include
   rapid edits and repeated document switching. Use synthetic fixtures for
   repeatability and representative local documents for realism.

Rationale: ownership should concentrate lifecycle rules in small interfaces;
performance enforcement must cover both predictable work bounds and measured
interaction costs without treating noisy wall-clock timings as unit tests.

## Execution sequence for final confirmation

1. Audit the selected paths and establish baselines before production edits.
   Check existing failures and distinguish CPU/headless measurements from
   actual input, presentation, and visual acceptance.
2. Refactor document/search ownership in small, behavior-preserving steps.
   Preserve stale-result protection, resource cleanup, unsaved-change handling,
   Markdown source, and host-agnostic, cross-platform crate interfaces.
3. Change editor layout only where audit evidence supports clearer ownership
   or identifies a measured bottleneck within the agreed scope.
4. Add targeted deterministic regression checks and a reproducible performance
   workflow. Choose numerical budgets from measured baselines; do not invent
   latency guarantees before measuring.
5. Run formatting, strict workspace/all-target Clippy, workspace tests, and
   affected performance benchmarks. Report before/after evidence and any
   platform, human-input, or visual checks that remain unverified.

Avoid feature removal, broad rewrites, and speculative abstraction layers.
Both decision rounds and final shared understanding are confirmed; implementation
is authorized within this scope.
