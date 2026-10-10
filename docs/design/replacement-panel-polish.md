# Replacements panel polish — design interview

Status: interview complete and implemented, 2026-10-10. Accepted in
[ADR 0033](../adr/0033-replacements-panel-redesign.md) and
[ADR 0034](../adr/0034-pseudonymization-supported-feature.md). Follows the popup redesign in [ADR 0032](../adr/0032-replacement-popup-redesign.md).

## Confirmed

### D1 Undo in the panel (Q1)
- Toolbar row 1 gets **Undo** and **Redo** icon buttons, disabled when history is empty.
- They are the **document undo**: the single editor history that already carries
  every mapping change. No separate replacement history.
- ⌘Z / ⇧⌘Z (Ctrl on Windows/Linux) work while focus is anywhere in the panel,
  including an empty search field.

### D2 Scan state lives only in the toolbar (Q2)
- Remove the ⋯ "Cancel scan" entry and the footer Cancel from the panel. The
  toolbar status (ADR 0032) is the only progress indicator and cancel.
- During a scan the panel keeps the current list; new proposals fade in on arrival.
- Empty panel during a first scan: quiet "Looking for names and identifiers…"
  (no spinner, no button). Empty after a scan: "Nothing found. Select text and
  choose Replace to add one."

### D3 Pseudonymization is no longer "experimental" (Q3)
- Drop "Experimental" from the panel footer and model setup copy.
- Reword README and ADR 0016 to present it as a supported feature; keep the
  measured qualification caveats as facts.
- Keep a permanent responsibility line: "Review for missed identifiers before sharing."
- Recorded in ADR 0034, which supersedes ADR 0016's "experimental until gates
  close" rule (Q22): the open gates become ongoing quality work, not release
  blockers. This is an assistive feature, not an anonymization guarantee.

### D4 Group expansion is independent of selection (Q4)
- Expansion is a per-group set, not derived from the selected entity.
- Clicking a group header toggles only that group: no selection, no popup, no
  other group collapses.
- Selecting a mention elsewhere (editor, popup ‹ ›) expands its group without
  collapsing others.
- Expansion persists for the document session; rescan keeps it, vanished groups drop.

### D5 Grey surfaces, coloured state text (Q5)
- Selected row: neutral grey fill, no coloured bar.
- Group header: stronger weight, own raised surface while expanded, ▸/▾ chevron and count.
- Mention rows: indented under the header, hairline dividers, original in muted grey.
- Colour only on state-naming text: alias in applied blue; snippet word in the
  chip's gold/blue.

### D6 Inline controls for panel selections (Q6)
- Selecting a mention row in the panel shows inline controls under it
  (Apply to: This one / Same text / All · Apply · Keep original).
- The editor scrolls to and outlines the mention but opens **no** popup.
- Selecting in the editor still opens the popup. Controls exist in one place at a time.

### D7 Second toolbar row (Q7)
- Right of search: **Expand/Collapse all** toggle and a **filter** icon button
  with a menu: All / Proposed / Applied.
- No Add button (Replace on a selection covers it), no sort (document order).

### D8 Rescan (Q8)
- The ⋯ menu becomes a single **Rescan** icon button on row 1.
- "Model settings…" moves to the app gear Settings (model sections live there).
- **No confirmation dialog** (Q15): rescan is non-destructive — existing
  variants, renames, links, keeps and mention moves survive; only new wordings
  are added — and it is cancellable from the toolbar.

### D9 Panel keyboard (round 2 input)
User-specified:
- ↑/↓ move between visible rows (group headers and mention rows).
- Move modifier + ↑/↓ starts a keyboard drag of the focused row or group; it
  steps through drop targets while held; past the end it reaches "New alias".
  Releasing the modifier drops; Esc while held cancels.
- Enter on a group header toggles it.
- On a mention row: Enter = Apply (This one), ⌘Enter = Apply (Same text),
  ⇧⌘Enter = Apply (All). Delete/Backspace = Keep original (This one),
  ⌘Delete = Same text, ⇧⌘Delete = All.

### D10 Move modifier (Q9)
- ⌥↑/↓ (Alt on Windows/Linux) moves the focused row/group; ⌘↑/↓ jumps to the
  first/last row.

### D11 Keyboard move semantics (Q10)
- Mention row → moves that one mention to the target entity. Group header →
  merges the entity into the target. "New alias" (end of list) accepts only a
  mention whose entity has more than one mention.
- While held: grey drop outline on the target group plus "Link to ALIAS" hint;
  the list does not reorder until release. Releasing on its own group is a no-op.
- One undo step per move. At either end the move stops at the last valid target
  (no wrap).

### D12 One shortcut scheme for panel and popup (Q11)
- Enter / ⌘Enter / ⇧⌘Enter apply This one / Same text / All; Delete(Backspace) /
  ⌘Delete / ⇧⌘Delete keep original in the same scopes — in both the panel and
  the popup. The Apply to selector only drives the buttons.
- Buttons show their shortcut ("Apply ↵", "Same text ⌘↵").

### D13 Applied rows and headers (Q12)
- Applied mention: Delete variants = Undo (back to proposed) in that scope;
  Enter does nothing.
- Group header: Enter toggles; ⇧⌘Enter / ⇧⌘Delete act on the whole entity (All);
  plain and ⌘ Delete/Enter do nothing else on a header.

### D14 Focus after a keyboard decision (Q13)
- Moves to the next undecided mention in document order, expanding its group.
- Groups expanded by this triage collapse once fully decided; groups the user
  expanded stay open.

### D15 Arrow keys follow through (Q14)
- Each ↑/↓ on a mention row shows its compact one-line inline controls and
  scrolls/outlines the editor mention.
- ↓ from search enters the list; ↑ from the first row returns to search; Esc in
  the list returns to search, then closes the panel.

### D16 Scroll wheel (Q16)
- Mouse wheel (line deltas) over the panel list steps the selection one row per
  notch, like ↑/↓. Trackpad (pixel deltas) scrolls the list normally.
- The list auto-scrolls to keep the selection visible; the editor follows (D15).

### D17 Middle click (Q17)
- Acts on the row under the pointer: Keep original (This one); ⌘ = Same text,
  ⇧⌘ = All. Applied rows: Undo in that scope. Group headers: nothing.
- Middle click on an editor chip does the same for that mention.
- One undo step each; the existing fade-out ghost plays.

### D18 Footer undo affordances become a transient notice (Q18)
- "Undo applying replacements", "Kept N · Undo keeping" and "Added X · Cancel
  addition" become a short-lived notice after the action ("Applied 5 · Undo")
  that fades after a few seconds. Toolbar Undo is the permanent affordance.
- Footer keeps "N mentions to apply · Apply replacements" and the
  responsibility line.

### D19 Alias and category from the panel (Q19)
- The alias on a group header is click-to-edit (entity-wide rename); a ▾ beside
  the category recategorizes the entity. Per-mention changes use moves (D11) or
  the editor popup.

### D20 Filter scope (Q20)
- Filters panel rows only; editor chips are never hidden. Remembered for the
  document session. Expand/Collapse all acts on visible groups. A dot marks a
  non-All filter.

### D21 Records (Q21)
- ADR 0033 (panel redesign), ADR 0034 (supported feature); glossary updated in
  CONTEXT.md.

## Invariants
- One document history: every panel action, move and keep is one undo step on it.
- One place for controls at a time: panel selection → inline controls, no popup;
  editor selection → popup.
- The same keys mean the same scope on both surfaces.
- The editor never hides identifying text because of a panel filter.
