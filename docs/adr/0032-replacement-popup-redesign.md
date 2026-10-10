# Replacement popup redesign and replacement motion

Status: Accepted and implemented, 2026-10-10. See the
[design interview](../design/replacement-popup-polish.md).

## Problem

The replacement popup shows every control at once in the same flat style:
counts repeat on every chip and button, the original and alias look unrelated,
scope chips carry no verb, and pickers expand inline until the popup becomes a
long list. Two ↶ icons mean different things. Apply and scanning give no visual
feedback in the document itself.

## Decision

- **Decision card.** Default rows: header (category ▾, `‹ n of N ›` hidden for
  one mention, ×), `original → [alias]` with a boxed alias input, an optional
  owner row (contact/identifier categories or when set), `Apply to` segmented
  scope control (the only place counts appear), and actions: filled `Apply`
  (↵ hint when Enter applies) plus ghost `Keep original`, or `Cancel` while a
  manual addition is the latest step, or labelled `↶ Undo` when applied.
- **Floating dropdowns** for category (name + resulting alias, ✓ current),
  alias ("Same as…" entities, `New alias`, `Rename to "X"` in the selected
  scope) and owner. The rename-all row and the header cancel icon are removed.
- **Replacement chips** in the editor: rounded, bordered, padding-free; gold
  proposals, blue applied.
- **Motion.** Apply: chip eases gold → soft blue glow → resting blue (700 ms,
  smoothstep) while the words roll inside the chip, for every apply path. Undo
  replacement eases blue → gold with the same roll; Keep original fades a
  ghost of the chip away. No marks. Scan: a toolbar status beside Pseudonymize (spinner + timed stage
  label crossfading Reading… → Analyzing… → Finding names… → Linking
  mentions…); proposals fade in when results land. Reduced motion disables
  the fades.

## Consequences

`mdoc-editor` gains a transient per-range annotation effect (colour/alpha over
time with frame-driven redraw) and bordered annotation painting. Renaming every
mention now requires choosing `All N` first. Scope, Keep/Undo and undo/redo
semantics are unchanged.
