# Replacements panel redesign

Status: Accepted, 2026-10-10. Superseded in part by
[ADR 0044](0044-panel-selection-and-tree-polish.md) (selection after a
decision, glyphs) and [ADR 0048](0048-workspace-shell-redesign.md) (panel
chrome, filter values, footer, scope keys). Supersedes the "neutral Enter never
applies" rule of ADRs 0020 and 0024.

## Problem

The panel lagged the popup redesign: undo worked only from the editor, scan
progress appeared in three places, groups expanded only through selection, a ⋯
menu hid Rescan, and decisions needed the popup or the mouse.

## Decision

- **Rescan** needs no confirmation: it keeps every decision and only adds new
  wordings. Model settings live in app Settings.
- **Scan state** is shown in one place (ADR 0032); the panel keeps its list and
  shows quiet empty states.
- **Groups** expand independently of selection; a header click only toggles.
  Colour appears only on alias and state text.
- **Keyboard**: ↑/↓ rows, ⌘↑/↓ first/last. Enter outside the alias field
  applies this mention as one undo step; in the alias field Enter only confirms
  the draft. ⌥↑/↓ is a keyboard move: step through target groups (then
  "New alias" for a mention of a multi-mention entity), release ⌥ to drop, Esc
  to cancel. Search ↔ list with ↓/↑; Esc returns to search, then closes.
- **Mouse**: wheel line deltas step the selection, trackpads scroll. Middle
  click keeps the original under the pointer (⌘ same wording, ⇧⌘ all; Undo on
  applied rows), in the panel and on editor chips.
- Contextual undo buttons become a transient "… · Undo" notice.

## Consequences

Expansion is per-group panel state. Keyboard move and middle click reuse the
existing mapping actions and add no new undo semantics.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0033-replacements-panel-redesign.md),
[interview](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/replacement-panel-polish.md).
