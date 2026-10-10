# Replacements panel redesign

Status: Accepted, 2026-10-10; not yet implemented. Interview record:
[replacements panel polish](../design/replacement-panel-polish.md). Builds on
[ADR 0032](0032-replacement-popup-redesign.md).

## Problem

The panel lagged the popup redesign. Undo worked only from the editor, scan
progress and cancel appeared in three places, groups expanded only through
selection (one at a time, auto-selecting a mention), selection used green and
blue fills, a ⋯ menu hid Rescan, and decisions needed the popup or the mouse.

## Decision

- **Toolbar.** Row 1: Undo, Redo (document history; ⌘Z/⇧⌘Z anywhere in the
  panel), Rescan (no confirmation: rescan keeps every decision and only adds
  new wordings), close. Model settings move to app Settings. Row 2: search,
  Expand/Collapse all, filter (All / Proposed / Applied; panel only).
- **Scan state** lives only in the toolbar status (ADR 0032). The panel keeps its
  list and shows quiet empty states.
- **Groups** expand independently of selection; a header click only toggles.
  Grey surfaces: grey selected fill, raised expanded header with chevron and
  count, indented mention rows with dividers. Colour only on alias and snippet
  state text. Header alias is click-to-edit; header ▾ recategorizes the entity.
- **Inline controls.** A mention selected in the panel shows one-line controls
  (Apply to · Apply · Keep original); the editor scrolls and outlines but opens
  no popup. Editor selections keep the popup.
- **Keyboard** (panel and popup share scopes): ↑/↓ rows, ⌘↑/↓ first/last,
  Enter / ⌘Enter / ⇧⌘Enter apply This one / Same text / All, Delete / ⌘Delete /
  ⇧⌘Delete keep original in the same scopes (Undo on applied rows). Enter on a
  header toggles; ⇧⌘ variants act on the whole entity. ⌥↑/↓ is a keyboard move:
  step through target groups (then "New alias" for a mention of a multi-mention
  entity), release ⌥ to drop, Esc to cancel. After a decision, focus goes to the
  next undecided mention. Search ↔ list with ↓/↑; Esc returns to search, then closes.
- **Mouse.** Wheel (line deltas) steps the selection; trackpad scrolls. Middle
  click keeps the original under the pointer (⌘ Same text, ⇧⌘ All; Undo on
  applied rows), in the panel and on editor chips.
- **Footer.** "N mentions to apply · Apply replacements" and the responsibility
  line; contextual undo buttons become a transient "… · Undo" notice.

## Consequences

Expansion becomes per-group panel state. The popup's Enter/Delete shortcuts
change to fixed scopes (the Apply to selector drives buttons only, which show
the matching key). This supersedes the "neutral Enter never applies" rule of
ADRs 0020 and 0024: Enter outside the alias field applies this mention, as one
undo step; in the alias field Enter still only confirms the draft. New keyboard
move and middle-click paths reuse the existing mapping actions and add no new
undo semantics.
