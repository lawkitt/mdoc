# Workspace Back/Forward

Status: Accepted, 2026-10-29. Amends
[ADR 0048](0048-workspace-shell-redesign.md) (title bar) and
[ADR 0077](0077-case-page.md) (case page).

## Problem

Moving between a case page, Home and several tabs has no way back: the
transient pages (Home, case page) vanish on the next tab activation, and the
user must find the previous place in the sidebar.

## Decision

- **Stops.** Each tab activation, Home and the case page (per case). Settings,
  scroll positions and mention selection are not stops. A closed tab's stops
  are skipped. The history is per window, about 50 stops, not restored at
  launch.
- **Keys.** macOS ⌘[ / ⌘]. Windows and Linux Alt+← / Alt+→ outside text
  fields; in the editor Alt+← stays Previous word (ADR 0061). Mouse buttons
  4 and 5 everywhere.
- **Buttons.** ‹ and › beside the sidebar toggle, disabled at either end, with
  tooltips naming the stop ("Back to Case “Raman”").
- The shortcuts panel (ADR 0073) lists both under Everywhere.

## Consequences

Back can reopen a case page without reading the case key (ADR 0077's invariant
holds).

References: [interview](../design/roadmap-grill-2026-10-29.md).
