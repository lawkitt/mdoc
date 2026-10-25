# Measured stabilization and architectural review

Status: Accepted, 2026-10-16. Amended by [ADR 0069](0069-app-architecture-and-code-ownership.md),
which defines the case-coordination and document-lifecycle projects.

## Problem

Spelling, encrypted cases, cross-document mapping updates, AI-reply restoration,
before/after views, updates and the new shell added overlapping refresh effects,
repeated computation and a quit path that discarded case-save errors. Tests
passed, but the ordinary host performance matrix failed and unchanged redraw
was expensive in debug/headless measurements.

## Decision

- Preserve current features and interaction design. Judge refactoring by clearer
  ownership, explicit effects, correctness and measured cost; file length alone
  never justifies extraction.
- **Performance tiers**: ordinary legal documents are the hard interactive
  target. Large inputs (2 MiB / 20,000 mentions) are a stress tier with bounded
  work and memory, without the same latency commitment. Numerical budgets come
  from release/native baselines at recorded viewports; debug/headless timings
  expose costs but establish neither native latency nor release regressions.
  `MDOC_PERF_STRESS=1` enables the stress tier.
- **Architecture**: case coordination and document lifecycle are separate,
  incremental projects that move domain transitions into explicit owners in the
  existing modules; GPUI keeps presentation, prompts and scheduling;
  editor/history, import and preview owners stay. Each extraction's boundary is
  defined before implementing it. Retained editor layout is investigated only if
  release-build cost justifies its complexity and geometry/editing risks.
- **Quit** waits asynchronously for case saves. A failure keeps the window and
  mappings and offers **Retry / Cancel / Quit without saving mappings**. Every
  path that abandons a quit clears all case-quit state; a failed write for a
  case removed mid-write still continues the other queued cases.
- **Derived review data** (panel projection, before/after view, spelling
  exclusions) is keyed on `Review::revision()`, which every `&mut` method of
  `Review` advances from one process-wide counter. `review_changed` only
  schedules spellcheck and the case save; paint-only syncs schedule nothing.
- **Spelling** cannot stay marked running: release builds abort on panic, and
  the worker handle is replaced only when no check runs. Keep both true.

## Invariants

ADRs 0010, 0011, 0021 and 0029 govern behavior and test preservation. Keep
source/history ownership, provenance, stable aliases, dirty state,
cancellation/revision guards, worker-owned admission, preview resource
lifetimes, encrypted case storage and stale-write protection. Case-wide
synchronization edits each member's own history; no other undo owner. Do not
raise failing ceilings to make the tree pass. No feature redesign, service
framework or text-storage replacement follows from this decision.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0049-measured-stabilization-and-architecture-review.md),
[audit](../evidence/stabilization/2026-10-16/README.md),
[results](../evidence/stabilization/2026-10-16/results.md).
