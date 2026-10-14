# Workspace polish from the Anoni review

Status: Accepted, 2026-10-14. Interview record:
[Anoni comparison](../design/anoni-comparison.md). Amends
[ADR 0030](0030-render-standard-markdown-only.md),
[ADR 0033](0033-replacements-panel-redesign.md),
[ADR 0041](0041-cases-and-ai-response-restoration.md) and
[ADR 0042](0042-live-before-after-view.md).

## Problem

A review of Anoni 0.81.1, a comparable local pseudonymization app, and of
beta.8 screenshots found three defects and several ideas worth taking:

- The "Detect in “case”" popup lets a long case name without spaces overflow
  the panel and push × outside it.
- DOCX tables convert to Markdown with `<br>` line breaks inside cells; mdoc
  shows them as literal text.
- The panel footer shows "In “case”, not in this document (0)".
- A restored reply that contains no placeholders reads "Restored 0 mentions of
  0 values as proposals…".
- The replacements panel lists entities in one flat list; Anoni groups them by
  category with counts, which makes long lists easier to scan.
- Nothing tells a new user that documents stay on the machine.
- The before / after view is reached only through a toolbar icon and has no
  single-column views.

## Decision

1. **Categories popup title.** "Categories to detect", the menu item's wording,
   with the case name below it on one muted line, ellipsized, × pinned right.
   Menu rows that embed a case name ("Move to case “…”") ellipsize too.
2. **`<br>` in table cells** renders as a line break. Only inside a table cell
   and only the exact forms `<br>`, `<br/>`, `<br />` (any case). Saved bytes,
   Copy Markdown and search are unchanged. A narrow exception to ADR 0030.
3. **Zero-count footer.** With nothing to list, "In “case”, not in this
   document (0)" becomes "Add a name to “case”", which still opens the field
   for adding a name to the case.
4. **Restore summary with nothing restored.** When a pasted reply has no
   placeholder at all, the bar says "No placeholders found in this reply." The
   existing bar (live unrecognized count, Next unrecognized, ×) is otherwise
   kept, including its instruction to apply before copying back, so it does
   not fade on its own.
5. **Category groups in the replacements panel.** Entities are grouped under a
   category header "Person 3 (7)": entities, then mentions in brackets. Fixed
   category order (the order of the category menu), empty groups hidden,
   entities in order of first appearance inside a group. Category headers are
   not collapsible and not focusable; keyboard navigation and the selection
   skip them. Search and the All / Proposed / Applied filter apply within
   groups; a group with no visible entity is hidden. No toggle.
6. **Local note on the empty page.** A lock icon and "Documents stay on this
   Mac" ("this computer" on Windows and Linux), with a hover explanation:
   "Detection, conversion and saved cases run on this Mac. mdoc goes online
   only to check for updates and to download models you choose."
7. **View switcher.** While replacements are applied, a floating segmented
   control at the bottom centre of the document offers **Edit · Original ·
   Pseudonymized · Side by side**. Original and Pseudonymized are the two
   columns of the ADR 0042 view shown alone, full width; Side by side is the
   ADR 0042 view. The toolbar icon and ⌘\ keep toggling Edit ↔ Side by side.
   ADR 0042's behaviour (live, read-only, Esc closes, PDF preview hidden,
   panel navigation) applies to all three views. No Reply mode: a restored
   reply is its own tab. No zoom.

## Not adopted

Per-category highlight colours, realistic fake names (they collide with real
text and defeat lenient restore; ADR 0018), a re-identification risk score, a
compliance report, and a Drop → Check → Export stepper (it fights the
editor-first workspace of ADR 0020). Key storage stays as ADR 0045 decided:
Anoni keeps its key in a plaintext file beside the encrypted data, which
removes prompts only by removing the protection.

## Invariants

- The view switcher never offers a view that differs from the editor's text and
  the applied replacements (ADR 0042).
- Category headers carry no state and accept no input.
- `<br>` handling is display-only.

## As implemented

- **`<br>`** (`mdoc-editor` `tables.rs`): a cell is shaped as its
  `<br>`-separated lines stacked top to bottom; row height, caret, click
  hit-testing and column measurement all use the same split. A caret inside a
  tag sits at the end of the line before it. The whole cell is styled before
  it is cut at the tags, so bold or a link spanning a `<br>` keeps its
  formatting on both lines (Codex review).
- **Restore summary:** the bar already existed (ADR 0041); only its empty case
  changed, to "Nothing to restore · No placeholders found in this reply."
- **Category groups:** entities keep the panel's existing order inside a group
  (identity creation order, which is detection order). The label row has the
  panel's fixed row height, with the label at its foot, so it also spaces the
  groups.
- **Local note:** tooltips are now capped at 320 px and wrap.
- **View switcher:** Original and Pseudonymized reuse the ADR 0042 view with
  one column; the column not shown stays laid out, invisible, so its scroll
  keeps following and switching columns keeps the reading position (Codex
  review). The floating Close stays. The document and the columns get
  extra bottom padding so the control never covers the last lines.
