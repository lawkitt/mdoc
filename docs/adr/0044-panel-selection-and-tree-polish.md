# Replacements panel: stable selection and tree polish

Status: Accepted, 2026-10-13. Supersedes ADR 0033's "after a decision,
focus goes to the next undecided mention" and its group disclosure glyphs. Row
layout and checkboxes amended by [ADR 0048](0048-workspace-shell-redesign.md).

## Decision

1. **Selection stays put.** Apply, Keep and Undo leave the cursor on the mention
   acted on, for mouse buttons and keyboard (Enter / Delete and their scope
   variants) alike. The group stays expanded. Keyboard triage is now Enter, ↓,
   Enter. `triage_next` and triage-expanded auto-collapse go away.
2. **After Keep** the mention leaves the list, so the cursor moves to its group
   header. The same happens when the panel's filter (Proposed / Applied) hides
   the decided mention; when the filter hides the whole group, nothing is
   selected.
3. **Mention bar colour**: applied and selected = the applied blue
   (`theme.applied()`, as alias highlighting); applied and not selected = the
   same blue at about 45% opacity. Proposed mentions unchanged: white when
   selected, grey otherwise. Blue means only "applied".
4. **No disclosure glyphs.** Group headers drop ▸/▾. Expanded groups are
   recognised by their band and indented rows; collapsed headers get a hover
   background. The alias line sits flush under the name (no 14 px indent).
   The category dropdown's "▾" ("Person ▾") stays: it marks a menu.
5. **One indent level.** The mention bar aligns with the header text's left
   edge (12 px); the snippet starts 10 px after the bar; the inline controls
   (This / Same / All, Apply, Keep) align with the snippet.
6. **No repeated original.** A mention row shows its small original line only
   when the wording differs from the group header (a variant); otherwise the
   row is just the snippet.
7. **One undo per state.** The row's ↶ icon is hidden while the row is
   selected (the controls' **Undo** remains); it still shows on hover for
   unselected applied rows.
8. **Close button** in the panel (and the chooser) uses the Original viewer's
   close control, `✕` with its size and hit area, from one shared helper so the
   two cannot drift. Other panel header icons are unchanged.
