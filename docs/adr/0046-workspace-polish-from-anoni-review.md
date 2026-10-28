# Workspace polish from the Anoni review

Status: Accepted, 2026-10-14. Amends [ADR 0030](0030-render-standard-markdown-only.md),
[ADR 0033](0033-replacements-panel-redesign.md),
[ADR 0041](0041-cases-and-ai-response-restoration.md) and
[ADR 0042](0042-live-before-after-view.md). Its "no per-category colours" and
the switcher placement are superseded by [ADR 0048](0048-workspace-shell-redesign.md).

## Problem

A review of Anoni 0.81.1, a comparable local pseudonymization app, and of beta.8
screenshots found layout defects (overflowing case names, literal `<br>` in
DOCX tables, zero-count notices) and ideas worth taking: category grouping, a
local-only note and more before/after views.

## Decision

1. **Categories popup title.** "Categories to detect", with the case name below
   on one muted, ellipsized line and × pinned right. Menu rows that embed a case
   name ("Move to case “…”") ellipsize too.
2. **`<br>` in table cells** renders as a line break: only inside a table cell
   and only `<br>`, `<br/>`, `<br />` (any case). A cell is shaped as its
   `<br>`-separated lines stacked; row height, caret, hit-testing and column
   measurement use the same split. The cell is styled before it is cut, so
   formatting spanning a `<br>` continues; a tag inside a code span or after a
   backslash stays literal. A narrow exception to ADR 0030.
3. **Zero-count footer.** With nothing to list, the case footer reads "Add a
   name to “case”" and still opens the field.
4. **Restore with nothing restored** reads "Nothing to restore · No placeholders
   found in this reply." Otherwise the bar reads "AI reply restored · The N
   restored names are proposals. Apply turns them back into aliases before
   you send the text to the AI again." (one count, the mentions; singular "The
   restored name is a proposal…"), then the unrecognized-placeholder sentence.
5. **Category groups** in the Replacements panel: fixed category order (the
   category menu's), empty groups hidden, entities in identity creation order.
   Category headers are not collapsible or focusable; keyboard navigation and
   selection skip them. Search and filters apply within groups; a group with no
   visible entity is hidden.
6. **Local note**: a lock icon and "Documents stay on this Mac" ("this
   computer" on Windows and Linux), explained on hover: "Detection, conversion
   and saved cases run on this Mac. mdoc goes online only to check for updates
   and to download models you choose." Tooltips are capped at 320 px and wrap.
7. **Views**: **Edit · Original · Pseudonymized · Side by side** while
   replacements are applied. Original and Pseudonymized are the ADR 0042 view
   with one column; the hidden column stays laid out so scrolling and switching
   keep the reading position. ADR 0042's behaviour applies to all three. No
   Reply mode (a restored reply is its own tab), no zoom.

## Not adopted

Realistic fake names (ADR 0018), a re-identification risk score, a compliance
report, a Drop → Check → Export stepper (it fights the editor-first workspace),
and Anoni's plaintext key file (ADR 0045).

## Invariants

- No view ever differs from the editor's text and applied replacements.
- Category headers carry no state and accept no input.
- `<br>` handling is display-only: saved bytes, Copy Markdown and search are
  unchanged.

References: [interview](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/anoni-comparison.md).
