# Direct replacement workspace

Status: Accepted, 2026-10-08. Superseded in part by
[ADR 0023](0023-pseudonymization-ui-polish.md) (layout, one popup scope),
[ADR 0032](0032-replacement-popup-redesign.md) (popup),
[ADR 0033](0033-replacements-panel-redesign.md) (Enter applies) and
[ADR 0048](0048-workspace-shell-redesign.md) (panel and popup chrome).

## Problem

The panel and the document popup duplicated editors, linking hid behind menus,
an existing alias produced a dead collision error, and selecting a panel item
lost the word in the document.

## Decision

- **One workspace**: the panel is a searchable overview, the word popup is the
  routine editor. Panel and document clicks reveal the active occurrence and
  open the same controls. A group click returns to its last valid occurrence,
  else the first in document order.
- **Same alias means same entity.** "Belongs to" (ownership) is a separate
  relationship with separate aliases.
- **Linking scopes**: this mention, same wording (matching originals currently
  assigned to the entity, respecting separated homonyms and Keep), entire
  entity, with real affected counts. Joining other variants needs a deliberate
  broader operation.
- **No Save alias**: choosing an existing alias or confirming a new one records
  the mapping. Typing alone is a draft. Pending originals await Apply; applied
  corrections update text at once through undoable operations.
- **One alias chooser** searches aliases and original variants, suggests likely
  entities by original name, and creates aliases inline (next free token or
  custom). Entering an existing alias offers linking, never an error. A new
  alias affects the selected scope; renaming a whole entity is explicit.
- **Category correction** keeps custom aliases, replaces generated
  wrong-category tokens and keeps unrelated aliases stable. Cross-category
  linking adopts the destination category for the moved mentions only.
- **Hidden-source occurrences** show their containing passage and the labelled
  exact value; they never claim a rendered word is the occurrence.
- **Focus and Escape**: popups open with neutral focus. Escape dismisses
  suggestions, then the draft, then the popup (keeping the panel). Switching
  items discards unconfirmed typing without a dialog. Invalid input is
  explained beside the field.
- Linking, renaming and applying keep the active occurrence and popup.

## Invariants

Stable aliases, occurrence provenance, tab-lifetime originals (without a case),
undo/redo, Keep and revert scope, syntax protection, stale-result guards and
exact Copy Markdown.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/5cad017e5a38efe1f009d47f54d1f0d9e2995547/docs/adr/0020-direct-replacement-workspace.md),
[interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-direct-interaction.md).
