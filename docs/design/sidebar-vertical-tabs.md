# Chrome-style vertical document sidebar — 2026-10-09

Status: shared understanding confirmed and implementation authorized by the
user's "Implement" on 2026-10-09. Implemented locally; verification below.
Recorded as [ADR 0027](../adr/0027-chrome-style-vertical-sidebar.md).
This revision revisits the sidebar portion of
[the sidebar and toolbar revision](ui-chrome-revision.md).

## Evidence

User screenshots compare the current expanded sidebar (header with ‹, "Documents",
and +) and collapsed rail (›, +, per-document format entries) with Chrome's
vertical tabs: expanded panel with a full-width + row below the last tab, and a
collapsed icon rail that slides the full panel out over the page on hover.

Current code (`src/tabs.rs`, `Tabs` sidebar render): fixed 200px expanded / 40px
collapsed widths; collapsed is the default for any document count; explicit
`ToggleSidebar` choices persist in the session. Clicking a collapsed entry opens
the **document list popup** (300px deferred panel) rather than activating it.
GPUI `with_animation` is already used elsewhere (`src/ui.rs`, PII popups).

## User direction

1. Default is expanded ("uncollapsed"); the + control moves to the bottom.
2. When collapsed, hovering slides the sidebar out with an animation.

## Design tree

1. Default and persistence — settled (Q1).
2. Expanded layout: + placement and header — settled (Q2, Q3).
3. Collapsed hover reveal vs the document list popup — settled (Q4).
   - Hover timing and dismissal — settled (Q7, Q8).
   - Keyboard reveal — settled (Q9).
4. Toggle animation — settled (Q5).
5. Width/resizing — settled, out of scope (Q6).

## Decisions

Round 1 — the user answered "agree" to every recommendation:

- **Q1 Default.** With no saved choice the sidebar starts expanded. Explicit
  toggle choices remain persisted and restored; opening/closing documents never
  changes them. Supersedes the collapsed default of the previous revision.
- **Q2 New row.** The header + is removed. Expanded: a full-width + row directly
  under the last document, scrolling with the list (Chrome behavior). Collapsed:
  a square + below the compact entries. Native New menu item and shortcut remain
  the always-reachable path.
- **Q3 Header.** Only a single SVG sidebar-toggle icon (same monochrome outline
  family as toolbar icons) with tooltip and accessible name. No "Documents"
  label, no ‹/› glyphs.
- **Q4 Hover reveal.** Hovering the collapsed rail slides out the full expanded
  panel as an overlay above the editor (no content reflow). It replaces the
  document list popup entirely. Compact entry clicks activate their document
  directly. The overlay is the ordinary expanded list: close ×, context menu,
  drag reorder, and + row all work.
- **Q5 Toggle animation.** Explicit expand/collapse animates width 40↔200px with
  the editor reflowing, using the same ~150ms ease-out as the hover reveal.
- **Q6 Width.** Fixed 200px expanded; drag-resizing is out of scope.

Round 2 — the user answered "agree" to every recommendation:

- **Q7 Reveal timing.** ~200ms hover intent on the collapsed rail, then a ~150ms
  ease-out slide 40→200px. Overlay carries shadow and right border.
- **Q8 Dismissal.** ~300ms grace after the pointer leaves, then a ~150ms slide
  back; re-entering cancels. The reveal stays open while a tab context menu or
  drag is active. Activating a document or + keeps it open until the pointer
  leaves.
- **Q9 Keyboard.** Focus entering any rail control reveals immediately (no
  delay); it stays while focus is within. Escape dismisses and returns focus to
  the originating rail control; focus leaving dismisses. This replaces the
  popup's Escape/focus-restoration behavior.

## Invariants

- The hover reveal never changes or persists the sidebar choice; only the toggle
  (button, `ToggleSidebar`) does.
- The overlay never reflows the editor; the explicit toggle does (animated).
- Preserve tab lifecycle, duplicate-name disambiguation, close handling, drag
  reorder, context menu, active/status indication, tooltips on compact entries,
  and keyboard traversal.
- Tests covering the document list popup are replaced by equivalent hover-reveal
  coverage (open/dismiss, direct activation, focus restoration, closed-opener
  case).

## Terminology

**Collapsed rail**: the 40px vertical sidebar (unchanged meaning).
**Hover reveal**: the temporary expanded panel overlaid from the collapsed rail;
it never changes the persisted sidebar choice. Replaces **document list popup**.

## Implementation and verification — 2026-10-09

- `Tabs` replaces the document list popup state with a `Reveal`
  (Hidden/Shown/Hiding) state, hover flags for the rail and reveal, and
  ticketed detached timers so only the latest open/close timer acts.
- The reveal is a deferred overlay in a tab group with index −1, so its
  controls precede the editor in Tab order like the rail it covers; rail
  controls leave the tab order while it is shown. The tab context menu is
  deferred at a higher priority so it paints and receives clicks above the
  reveal (a regression test caught it being occluded).
- Escape focuses the originating rail control and suppresses re-revealing
  until focus moves elsewhere. Explicit toggles keep keyboard focus on the
  replacing toggle.
- Widths animate with GPUI animations (150 ms ease-out quint), which honor the
  reduced-motion preference; with reduced motion the reveal unmounts at once.
- Sessions without an explicit choice open expanded; the former inference of a
  collapsed choice from `sidebar_visible: false` was removed.
- New `resources/ui/sidebar.svg` toggle icon.

Automated validation: `cargo fmt`, workspace Clippy with warnings denied, and
`cargo test --workspace`: 458 passed, 16 ignored probes. New/rewritten
coverage: default and persisted choice (including sessions without a choice),
direct compact activation, hover intent and pass-through, grace period and
re-entry cancel, reveal never pinning, reveal toggle pinning, + row placement
expanded and collapsed, context menu holding the reveal and closing a tab,
keyboard reveal from entries and toggle, Escape focus restoration, closing the
opener tab, and Tab reaching every document.

Native macOS check (dark theme, three blank tabs): collapsed rail with entries
and square +; hovering slides out the full list over the editor; it remains
during the grace period after leaving, then slides back; the reveal's toggle
pins the sidebar and the editor reflows. The original session file was restored
byte-for-byte afterwards. Not checked natively: light theme, keyboard reveal,
drag reorder within the reveal, and Windows/Linux.
