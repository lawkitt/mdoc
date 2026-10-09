# Keep editor input and edit history together

Status: accepted and implemented locally, 2026-10-05. After the Workspace cleanup
in [ADR 0010](0010-workspace-construction-and-test-parity.md), the user requested
"Ok, now finish refactoring", authorizing the deferred focused editor extraction.

## Problem and decision

EditorState's root implementation interleaves source mutation, undo grouping,
clipboard and keyboard editing, UTF-16 conversion, pointer navigation, painted
geometry and menu rendering. Reviewing an edit's history/revision effects or an
IME conversion requires finding related methods spread across the root file.

Put source replacements and loads, diagnostic remapping, auto-replacement,
keyboard edit/format/clipboard actions, history snapshots/coalescing and the
EntityInputHandler implementation in a private `input` module. Keep UTF-16 and
grapheme conversion helpers beside their input consumers. Use explicit imports
and expose helpers to the parent only where existing callers require them.

EditorState remains the sole owner of source, selection, history, providers and
caches. Public types and methods keep their existing crate paths. Pointer
navigation and painted geometry remain with the existing rendering code; tables
continue to use the same edit-history methods. This introduces no new service,
trait hierarchy, runtime indirection or independently synchronized state.

The practical benefit is one place to review and test input behavior. File-size
reduction is incidental. This completes the targeted cleanup selected in the
maintainability review (removed in `d10fba0`; see ADR 0010). Lifecycle state
redesign, further rendering splits, API/feature removal and a generalized
artifact framework have no demonstrated need in this review.

## Invariants and verification

Preserve source bytes, caret/selection, notification order, content revisions,
stale replacement rejection, diagnostic remapping, undo grouping and limits,
redo branching, clipboard routing, auto-replacement and IME conversion behavior.
This extraction does not change or claim to repair existing IME semantics.

All 45 extracted function signatures and bodies were compared against the
committed source as Rust tokens: only internal visibility and placement changed.
The existing word-boundary test moved beside its helper. Two additional GPUI
regressions exercise Unicode typing/selection/clipboard undo groups and redo
branching, plus UTF-16 ranges through composition, commit, undo and source load.
Existing grouped replacement, annotation geometry, table editing, application
source/save/undo and lifecycle regressions remain in their original suites.

Verification covers formatting, strict workspace Clippy, the full workspace
suite, the production development build and the existing ignored host performance
matrix at its unchanged source workloads, viewport and ceilings. Headless input
tests do not establish native IME, clipboard, visual or Windows/Linux acceptance.
Native limitations documented in
ADR 0010 remain applicable; this extraction does not address them.

## Results

- The focused input suite passes all three tests, including the two new
  regressions. The complete workspace test suite passes.
- Formatting, strict workspace Clippy and diff whitespace checks pass. Cargo's
  registry setting warning and upstream `block` future-compatibility warning
  remain unchanged.
- The ignored `host_performance_matrix` passes all six synthetic workloads and
  twelve preview/resource-release cycles in about 70 seconds. This is a
  headless regression check, not a measured native latency improvement.
- The production development build passes. No dependency or reusable feature
  was removed; the new module and follow-up documentation are local changes.
