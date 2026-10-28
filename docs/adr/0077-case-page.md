# Case page

Status: Accepted, 2026-10-28. Amends
[ADR 0041](0041-cases-and-ai-response-restoration.md) (Open Case…, case
actions) and [ADR 0048](0048-workspace-shell-redesign.md) (case header ⋯, case
colour).

## Problem

Nothing gives an overview of a case. The sidebar header truncates long names,
the Share panel's Case row only says "saved", the colour is derived from the
id and can't be changed, and Open Case… opens every member document at once.

## Decision

- **A page in the content card, like Home.** It opens from a click on the case
  header, Home's Cases list, the Share panel's Case row, the palette and
  **File → Open Case…**, which now opens the page rather than every document.
  The breadcrumb reads "Case", and the case header shows selected while the
  page is open. The page is transient, like Home: it is not a tab, is not
  restored at launch, and the next tab activation replaces it.
- **Header.** A colour dot (click to open the swatches), the name (click to
  rename inline), and "N documents · M names · created 12 Oct".
- **Documents.** Every member, with its status: open, closed or missing. A
  click opens or activates it. Each row has **Remove from case**, which is the
  only action for a missing file. **Open all** opens every member.
- **Names.** The case's identities grouped by category: alias, primary wording
  and number of variants, plus mention counts for open members ("—" for closed
  ones; closed members' files are not read). There is a filter field once
  there are more than 10. **Forget identity** (confirmed, irreversible) and
  **Add a name** are here. Editing wording, category and merges stays in each
  document's Replacements panel.
- **Categories to detect** is a section of the page.
- **Delete case and forget originals…** sits at the bottom, with the existing
  confirmation. Afterwards the page closes to Home and member tabs stay open
  without a case.
- **Case ⋯.** Rename, Categories to detect and Delete open the page at their
  place. Close all stays.
- **Case colour.** Eight named swatches: Red, Orange, Amber, Green, Teal, Blue,
  Violet, Pink. Each passes the [ADR 0067](0067-theme-contrast-and-type-ramp.md)
  contrast test in both themes. The colour is stored on the case
  (`serde(default)`); a case without one keeps its derived hue, snapped to the
  nearest swatch. The same colour shows on the sidebar dot, the breadcrumb,
  Home's Cases list and the page.
- **Locked case.** The page shows the name, the document count and
  **Unlock…** only. Opening the page never reads the key.

## Invariants

- Opening the case page never reads the case key or a member file.
- Category colour rules of ADR 0048 stand: a case colour marks the case, never
  a state.

## Consequences

Opening a closed case takes a second click (Open all, or a document), which
keeps a large case from flooding the sidebar. Workspace Back/Forward (Later)
can return to the page.
