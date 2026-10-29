# Replace a custom selection

Status: Accepted, 2026-10-09. Amended by
[ADR 0051](0051-roadmap-structure-and-stable-scope.md) (repeats inside a longer
mention) and [ADR 0055](0055-stable-compact-mention-popup.md) (pill placement,
hidden when invalid). Superseded in part by [ADR 0025](0025-review-refinements.md) (category
guess) and [ADR 0033](0033-replacements-panel-redesign.md) (Enter applies).
Amended 2026-10-29 ([interview](../design/roadmap-grill-2026-10-29.md)):
the split pill.

## Problem

Detection misses identifying text. Manual addition exists (`Review::add_manual`,
⌘⌥P) but is hidden in the Replacements ⋯ menu, needs a category cycled
beforehand, and reports invalid selections only after the click.

## Decision

- **Entry points**: a floating `Replace` pill beside a settled selection while
  the tab has a review, a first "Replace with placeholder ⌘⌥P" right-click item
  (new host menu-item hook in `mdoc-editor`), and ⌘⌥P. Remove the ⋯-menu
  "Add selected text" and "Selection type" entries. Not offered before the first
  Pseudonymize; using it after Close review reopens review.
- **Split pill.** The pill reads "Replace · Person ▾": the main part adds
  with the guessed category (as ⌘⌥P does); ▾ opens the category list, with
  Other, and adds in the picked one. One line high; the same keyboard reach
  as the mention bar's category control.
- **One action, correct after**: add with a guessed category (ADR 0025); the
  popup opens on the new mention with Apply as the Enter default, and its
  category control fixes a wrong guess.
- **Scope and state**: all exact repeats become proposed (amber) mentions of one
  variant; popup scope chips narrow it. One undo step. A repeat inside a longer
  mention ("Raman" in "Priya Raman") stays part of it, and that mention's
  identity is suggested first under Same as…; nothing links automatically.
- **Tidy selections**: trim edge whitespace, punctuation and unambiguous edge
  Markdown delimiters. A selection fully containing only proposed mentions
  supersedes them. Anything else invalid disables the menu item with a
  one-line reason and hides the pill (ADR 0055).
- **No provenance marking**: manual additions look like any replacement and
  survive Rescan.

## Unchanged

Linking via normalized identities and the popup alias chooser, and the
Keep/Undo semantics of ADR 0023.

References: [interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-manual-selection.md).
