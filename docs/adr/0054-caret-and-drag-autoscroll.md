# Caret and drag-selection autoscroll

Status: Accepted, 2026-10-17.

## Problem

The editor auto-grows to its content and the host owns the scroll view. Only
host jumps (find, spelling, PII) scrolled it, so a mouse drag or Shift+arrow
selection, typing, or arrowing past the viewport edge left the caret off-screen.

## Decision

Follows Zed's editor (`scroll/autoscroll.rs`, `element/mouse.rs`).

- **Opt-in container.** A host passes its `ScrollHandle` to
  `EditorState::set_autoscroll`; the editor scrolls it. Every host scroll view
  that shows an editor opts in (document view, both compare columns).
- **Keyboard reveal.** Any keyboard-driven caret or selection change — the
  editing/moving actions, undo/redo, typed and IME text — keeps the caret in
  view after the next paint. Mouse clicks, copy, dismiss and host jumps do not.
- **Fit, not center.** Scroll only as far as needed to keep three base line
  heights of context above and below the caret row (fewer when the viewport is
  too short for that), Zed's `Fit` strategy with its default margin.
- **Drag autoscroll.** While a mouse selection is held and the pointer is
  within one line height of the viewport's top or bottom edge (or beyond it),
  the view scrolls every frame and the selection follows, even with a still
  pointer. The inner band keeps a window at the screen edge scrollable. Speed
  is Zed's curve: `(distance^1.2 / 100)` lines per step, capped at 3 lines.

## Invariants

- No reveal while the caret is already within the fitted range: no scroll
  happens on a keystroke away from the edges.
- Drag autoscroll stops on release, including release outside the editor.
