# Give document preparation a clear UI hierarchy

Status: Accepted, 2026-10-05. Its toolbar and sidebar are superseded by
[ADR 0027](0027-chrome-style-vertical-sidebar.md) and
[ADR 0048](0048-workspace-shell-redesign.md).

## Problem

Routine commands, review controls, model diagnostics and warnings competed for
attention and space; wide tables hid horizontal navigation; preview notices
repeated themselves.

## Decision

- The chrome prioritizes preparing documents for AI handoff while keeping
  Markdown editing.
- **Tables** fit columns by wrapping cells where readable, respecting saved
  widths, as a view-only change. Below usable minimum widths, horizontal
  scrolling is made obvious. Text is never shrunk to fit.
- **Source preview** is adjustable, with per-tab visibility and divider
  position. Where two readable panes do not fit, Markdown and Source switch.
- **Settings** leads with task-facing summaries and the selected model's
  installation state; technical details and comparison tools sit behind
  explicit disclosure.
- **Warnings** are scoped: preview warnings in the preview, review and handoff
  warnings with review.
- 640×480 is a supported viewport. Keyboard traversal and focus restoration
  work as normal; compact metrics stay within the palette and typography.

## Invariants

Visual simplification never implies qualified model quality or guaranteed
anonymization. Source, caret/selection, dirty and undo state, attachments and
per-document lifecycle are preserved.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/5cad017e5a38efe1f009d47f54d1f0d9e2995547/docs/adr/0012-document-preparation-ui.md),
[UI interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/ui-review.md).
