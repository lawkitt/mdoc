# Chrome-style vertical document sidebar

Status: Accepted, 2026-10-09. Supersedes the sidebar of
[ADR 0012](0012-document-preparation-ui.md). Widths, header and toggle animation
superseded by [ADR 0048](0048-workspace-shell-redesign.md).

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
- Sidebar width is fixed; resizing is out of scope.

## Consequences

Popup-specific state and tests are replaced by hover-reveal state, timers and
equivalent tests. Rendering gains hover-intent timers and width animations.

References: [interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/sidebar-vertical-tabs.md).
