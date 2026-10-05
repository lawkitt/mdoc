# Construct document workspaces with their production dependencies

Status: accepted and implemented locally, 2026-10-05. The user agreed to targeted
cleanup and preservation of reusable crates/features, then confirmed the exact
first-change scope and shared understanding with "Ok".

The subsequently authorized editor cleanup is completed separately in
[ADR 0011](0011-editor-input-module.md).

## Problem and rationale

Production constructs every Workspace inside Tabs with its owner, shared theme,
preferences, model panel and import admission state. The general constructor
first creates standalone defaults; Tabs then overwrites them. UI tests also
construct a standalone Workspace window. That second mode retains separate
document open/new/close, theme and OCR-install branches and makes test setup
different from the shipped application.

Make the production relationship explicit at construction and test it directly.
Keep existing service owners and reusable crates. The purpose is fewer lifecycle
paths and more representative tests; reducing file sizes is not an acceptance
criterion.

## Decision

- Require Workspace's tab owner and shared dependencies at construction, without
  introducing a trait hierarchy, general service container or new crate.
- Remove standalone fallbacks demonstrated to be unreachable through production
  construction. Route New/Open/Close, theme and Settings through the existing
  tab shell; keep document-owned import consent and completion behavior.
- Boot relevant UI tests through Tabs and select the active Workspace for local
  assertions. Exercise document/window transitions through the existing shell
  rather than retaining a test-only production lifecycle. Adapt affected preview,
  pseudonymization and performance fixtures while preserving their assertions
  and workload purpose. Do not remove tests simply to make the cleanup pass.
- Retain pure owner/session tests as unit tests. Test fixtures may supply
  deterministic settings, model status and completion results; tests must not
  require real downloads or a qualified detector.
- Limit production changes to the Workspace dependency relationship and affected
  callers. Editor extraction, lifecycle state-machine redesign, feature/API/crate
  removal, dependency updates and the optional hash-helper cleanup are deferred.

## Invariants

Preserve complete Markdown source and handoff, dirty state/save prompts,
caret/selection/undo, source attachments and preview reading state, tab identity,
lazy opening and conversion admission. Application settings/theme remain shared;
consent and results remain attributed to the initiating document/configuration.

Retain cancellation, generation/revision checks and worker-owned resource
permits. A closed tab or stale completion must not mutate another tab or release
a still-running worker's admission slot. Failed preview replacement must preserve
the displayed source. Preserve mappings while switching tabs and release them
when closing their tab. Preserve existing close/quit/session persistence behavior.

## Completion and verification

Review the resulting diff for actual removal of duplicated fallback behavior and
the absence of a new equivalent abstraction. Run focused tab/action, save/close,
import/OCR continuation, preview and pseudonymization regressions, then formatting,
strict workspace Clippy and the full workspace test suite. Run affected existing
ignored host/performance checks if their fixture or workload changes; do not
compare different workloads as equivalent timing evidence.

Perform an available native macOS smoke check of New/Open/Close, unsaved prompts,
tab switching, theme and Settings routing. Report unavailable checks explicitly;
automated success does not establish native or Windows/Linux acceptance. Deliver
a local reviewable diff; commit, PR publication, merge and release are outside
this first-change scope unless separately requested.

See [the quick investigation and decision tree](../design/maintainability-review.md).

## Implementation and verification

Workspace now requires a tab owner, shared settings/model panel, theme, import
admission state and captured OCR status at construction. Tabs supplies these
directly instead of replacing standalone defaults. Standalone New/Open/Close,
theme, direct OCR/PII setup, immediate DOCX worker and default OCR-check paths
were removed. Shared Settings status replaces standalone PII installation state.
Cancellation, generation checks, worker permits and preview replacement owners
remain intact. No reusable crate, feature, public library API or dependency was
removed.

The UI fixture now opens the tab shell and obtains its active Workspace. Former
single-document New/save/discard tests exercise the production Close prompt;
New assertions check a fresh active tab while the previous tab retains its
state. Canonical paths, duplicate-open activation, import identity replacement,
late completions after close, raw-source copy/undo, review mappings and preview
release assertions remain covered. Existing performance fixtures use the shell's
Open/Close paths. Their outer width is 1332px to preserve the original 1100px
document viewport beside the 232px sidebar; source workloads and regression
ceilings are unchanged. The first run at 1100px outer width failed the ordinary
workload's timing ceiling, demonstrating that it was not a comparable viewport.

Verification on this macOS host:

- Formatting and strict workspace Clippy pass. Cargo still reports its registry
  setting warning and upstream `block` future-compatibility warning.
- All workspace tests pass: 374 passed, 11 ignored, across 12 suites. The app-only
  suite passes: 149 passed, 8 ignored.
- The explicitly run ignored `host_performance_matrix` passes all six synthetic
  workloads and twelve preview/resource-release cycles in about 71 seconds.
  This includes sidebar/shell work; it is a CPU regression check, not a promise
  of native frame latency or a before/after speedup measurement.
- The production development build and diff whitespace checks pass.

A temporary native app was built from the same production sources with only
the session/settings storage namespace isolated. It opened a synthetic Markdown
file without restoring the user's session. Native New and typing changed the
window title to a dirty Untitled tab; Close showed Save/Cancel/Discard; Cancel
preserved the dirty tab; Discard returned to the source tab. Open showed the
native file picker and Cancel returned to the document. Cmd+, displayed the
shared Settings panel and Escape closed it; screenshots showed both retained
tabs and the active editor. Cmd+Q closed the smoke window, but the process remained
idle in AppKit's event loop. A process sample showed no model-admission shutdown
wait; the isolated process was then terminated. Native process-exit acceptance
is not established by this run.

Native coordinate input failed with `noWindowsAvailable`; theme mouse routing
was therefore verified only by the automated production-shell regression.
Ctrl+Tab/Ctrl+Shift+Tab did not visibly switch the active tab in this automated
native run, so that shortcut's native acceptance remains unverified. Its bindings
and implementation were not changed, and shell tab-cycle tests pass. Human mouse,
IME, complete visual acceptance and Windows/Linux checks remain outstanding.
The production development binary was rebuilt after the isolated smoke build.
