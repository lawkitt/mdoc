# Stable, compact, reversible mention popup

Status: Accepted, 2026-10-17; Enter and Apply all decided 2026-10-22. Amends
the popup, scope and keys of [ADR 0048](0048-workspace-shell-redesign.md).

## Problem

The mention popup (ADR 0048) blinked out and back while a mention was selected,
stepped or scrolled, sometimes framing a wrong position. At 720 px it was wider
than its controls, covered the lines under its mention, and its border vanished
over text. It defaulted to the "Same text" scope, so the whole entity took an
extra click. Enter applied only this mention and switched the scope to This
one, and ⌘↩ left the popup on a mention it had just applied
([#78](https://github.com/lawkitt/mdoc/issues/78)). Unticking Replace closed it, and kept mentions lost every mark, so
undoing a keep meant finding the mention again. The "Replace ⌘⌥P" chip covered
the selection's neighbouring lines.

## Decision

- **One stable popup.** One popup follows the active mention through panel
  selection, ‹ › stepping and scrolling, without dismissal, duplicates or
  frames at a stale position. While the mention is outside the viewport the
  popup hides and keeps its state; it returns when the mention does. Rapid
  selection jumps straight to the latest mention, with no animation or queue.
- **Two scopes.** The popup offers **This one | All N**; All is the whole
  entity and the default. "Same text" leaves the popup; ⌘⌫ (keep same wording)
  and the panel still reach it. The scope persists while stepping within the
  entity and resets to All when another entity is opened.
- **Enter applies in the selected scope.** With All N it applies every
  proposed mention of the entity, with This one only this mention; the
  segment does not change. Then the popup moves on: with This one to the
  entity's next proposed mention, otherwise (or when the entity has none left)
  to the first proposed mention of the next entity with proposals, in panel
  order. With no proposals left it closes. Enter on an applied or kept
  mention does nothing; ⌘Z and ticking Replace undo. After a manual addition
  Enter applies it and stays, so its Cancel stays offered (ADR 0024, 0025).
- **Apply all closes the popup.** ⌘↩ and the toolbar's Apply N apply every
  proposal in the document and close the popup; ⌘Z undoes them as one step.
- **Reversible Replace.** Unticking Replace keeps the mentions in scope and
  leaves the popup open, unticked. Ticking it again undoes that keep in the
  same scope. Each toggle is one undo step; text changes only on Apply. The
  panel's rule that ticking a kept mention proposes it alone is unchanged.
- **Kept marker.** While reviewing, a kept mention has a 1 px dotted underline
  in the muted text colour, with no category tint. Hover says "Kept · click to
  edit"; a click opens the popup on it with Replace unticked. After Close
  review kept text is plain.
- **Arrow keys.** In the popup (not in the alias field or an open menu), ← / →
  and ↑ / ↓ step to the previous / next mention of the entity, wrapping like
  ‹ ›; ⌘← / ⌘→ and ⌘↑ / ⌘↓ (Ctrl on Windows/Linux) go to the first / last.
- **One compact row.** The card fits its content, capped at the column, and
  wraps only in a narrow window; its left edge stays on the mention. 12 px
  controls and tight padding. The alias reads as plain blue text and shows its
  field only on hover or focus; the scope segments have no frame; the counter
  reads "1/5". ‹ › and × follow the scope.
- **Owner in the category menu.** For contacts and identifiers the category
  menu ends with "Belongs to…", which opens the owner picker in its place; a
  set owner shows on the chip as "Email · PERSON_1 ▾". No separate row.
- **Clear of the text.** The bar leaves the mention's line and the line below
  it visible, sitting about one line height under the mention, and flips above
  under the same rule. A 6 px notch on the card's edge points at the mention.
- **Selection chip.** "Replace ⌘⌥P" is a small pill just above the selection's
  end (below it near the window top): it lives only while the selection is
  held, so covering the line above is acceptable. It shows only when the
  selection can be replaced, with no tooltip; the right-click menu keeps the
  item and its one-line reason.
- **No hover tooltips** on mentions while the popup is open.
- **Border.** A 1 px neutral border one step stronger than `palette.border` in
  each theme, plus the shadow. State colour stays on the mention outline.

## Invariants

- Opening, stepping, scrolling or keeping never closes the popup; only Esc, ×,
  a click outside, or applying the last proposal (Enter or Apply all) does.
  Space and ⌫ keep like unticking Replace.
- Enter, Space, ⌫, ‹ › and the scope outline act on the segment shown; no key
  changes it, except opening another entity, which resets it to All.
- An occurrence's identity and the chosen scope survive stepping and toggling.
- Apply stays explicit; ticking never changes text.

## Consequences

ADR 0048's "One width (up to 720 px), just below its word", its three-segment
scope, its Space-unticks-and-closes behaviour and its "Enter applies this
mention" no longer hold, nor ADR 0024's disabled pill (the menu item keeps the
reason). Tests cover Enter in each segment, the move after it, and ⌘↩ closing.

References: [ROADMAP should-fix 1](../../ROADMAP.md#should-fix).
