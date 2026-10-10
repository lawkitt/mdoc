# Replace a custom selection

Status: Accepted and implemented, 2026-10-09. The Person fallback of the
category guess is superseded by [ADR 0025](0025-review-refinements.md). See the
[historical design interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/pseudonymization-manual-selection.md).

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
- **One action, correct after**: add with a guessed category (structured rule →
  phone shape → legal form → Person); the popup opens on the new mention with
  `Apply · N` as the Enter default, and its category chips fix a wrong guess.
  Enter applies only in a popup opened by a manual addition; elsewhere neutral
  Enter still never applies (ADR 0020).
- **Scope and state**: all exact repeats become proposed (amber) mentions of one
  variant; popup scope chips narrow it. One undo step.
- **Tidy selections**: trim edge whitespace, punctuation and unambiguous edge
  Markdown delimiters. A selection fully containing only proposed mentions
  supersedes them. Anything else invalid disables the pill/menu item with a
  one-line reason.
- **No provenance marking**: manual additions look like any replacement and
  survive Rescan.

## Unchanged

Linking via normalized identities and the popup alias chooser, live-tab
originals, Keep/Undo semantics and colours from ADR 0023.
