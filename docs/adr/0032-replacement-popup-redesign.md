# Replacement popup redesign and replacement motion

Status: Accepted, 2026-10-10. The popup layout is superseded by the mention
bar of [ADR 0048](0048-workspace-shell-redesign.md); chips, colours and motion
stand.

## Problem

The replacement popup shows every control at once in the same flat style:
counts repeat on every chip and button, the original and alias look unrelated,
scope chips carry no verb, and pickers expand inline until the popup becomes a
long list. Two ↶ icons mean different things. Apply and scanning give no visual
feedback in the document itself.

## Decision

- **Floating dropdowns** for category (name + resulting alias, ✓ current),
  alias ("Same as…" entities, `New alias`, `Rename to "X"` in the selected
  scope) and owner. The rename-all row and the header cancel icon are removed.
- **Applied chips** in the editor: rounded, bordered, padding-free, blue, text
  tinted to match. Proposed mentions are underlined (ADR 0048).
- **Motion.** Apply: chip eases gold → soft blue glow → resting blue (700 ms,
  smoothstep) while the words roll inside the chip, for every apply path. Undo
  replacement eases blue → gold with the same roll; Keep original fades a
  ghost of the chip away. No marks. Scan: a status beside the primary action (spinner + timed stage
  label crossfading Reading… → Analyzing… → Finding names… → Linking
  mentions…); proposals fade in when results land. Reduced motion disables
  the fades.

## Consequences

`mdoc-editor` has a transient per-range annotation effect (colour/alpha over
time with frame-driven redraw) and bordered annotation painting. Renaming every
mention requires choosing the whole-entity scope first. Scope, Keep/Undo and
undo/redo semantics are unchanged.

References: [interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/replacement-popup-polish.md).
