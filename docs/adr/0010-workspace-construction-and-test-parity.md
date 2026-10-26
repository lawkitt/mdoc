# Construct document workspaces with their production dependencies

Status: Accepted, 2026-10-05. Its "keep reusable crates and features" scope is
superseded by [ADR 0029](0029-codebase-cleanup-charter.md). Amended by
[ADR 0069](0069-app-architecture-and-code-ownership.md): GUI-free crates are allowed.

## Problem

Production built every Workspace inside Tabs, but the constructor first created
standalone defaults that Tabs then overwrote, and UI tests ran a standalone
Workspace window. That kept separate New/Open/Close, theme and OCR-install
paths and made tests differ from the shipped app.

## Decision

- Workspace requires its tab owner and shared dependencies (settings, model
  panel, theme, import admission, OCR status) at construction. No trait
  hierarchy or service container.
- Standalone fallbacks are removed; New/Open/Close, theme and Settings route
  through the tab shell. Document-owned import consent and completion stay.
- UI tests boot through Tabs and assert on the active Workspace. Fixtures may
  supply deterministic settings, model status and results; tests never need
  real downloads or a qualified detector. Tests are adapted, not removed, to
  make a cleanup pass.
- Fewer lifecycle paths and representative tests are the goal; file size is
  not an acceptance criterion.

## Invariants

Preserve complete Markdown source and handoff, dirty state and save prompts,
caret/selection/undo, source attachments and preview reading state, tab
identity, lazy opening and conversion admission. Settings and theme are shared;
consent and results belong to the initiating document and configuration.

Keep cancellation, generation/revision checks and worker-owned permits: a
closed tab or stale completion never mutates another tab or frees a running
worker's slot. A failed preview replacement keeps the displayed source.
Mappings survive tab switches and are released when their tab closes (unless
held by a case, ADR 0041). Close, quit and session persistence are unchanged.

Performance fixtures keep their document viewport (1100 px beside the sidebar);
different workloads are never compared as equivalent timing evidence.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0010-workspace-construction-and-test-parity.md),
maintainability review: `git show d10fba0^:docs/design/maintainability-review.md`.
