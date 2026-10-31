# Chrome-style vertical document sidebar

Status: Accepted, 2026-10-09. Supersedes the sidebar of
[ADR 0012](0012-document-preparation-ui.md). Default widths, header and toggle
animation superseded by [ADR 0048](0048-workspace-shell-redesign.md).

## Problem

The sidebar starts collapsed, hides its + in the header, and a collapsed entry
click opens a filename popup instead of switching documents. Users expect the
Chrome vertical-tabs model: expanded by default, + after the last tab, and a
collapsed rail that slides the full list out on hover.

## Decision

- **Default expanded** when no choice is saved; explicit toggle choices persist.
- **Hover reveal** replaces the document list popup: after ~200ms hover, the full
  list slides out (~150ms ease-out) over the editor without reflow. It
  closes ~300ms after the pointer leaves (re-entry cancels), stays open during a
  context menu or drag, and never changes the saved choice. Compact entry clicks
  activate directly.
- **Keyboard**: focus in the rail reveals immediately; Escape dismisses and
  restores focus to the originating rail control.
- **Width**: the expanded sidebar's edge drags between 160 and 320 pt (240
  until dragged); the width is saved in settings (`general.sidebar_width`)
  when the drag ends. Collapsing and expanding keep it.
- **Narrow windows**: below a 900 pt window width the sidebar collapses to the
  rail by itself, without the slide and without changing the saved choice,
  and expands again when the window widens, unless the user collapsed it.
  Expanded by the user in a narrow window, it stays until the window has
  widened; narrowing again collapses it by itself.
- **Names** truncate in the middle and keep their extension, so names that
  share a start still differ.

## Consequences

Popup-specific state and tests are replaced by hover-reveal state, timers and
equivalent tests. Rendering gains hover-intent timers and width animations.

References: [narrow windows](../design/ui-polish-grill-2026-11-01.md)
(decision 12), [interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/sidebar-vertical-tabs.md).
