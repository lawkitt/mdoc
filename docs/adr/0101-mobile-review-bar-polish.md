# Mobile review bar polish

Status: Accepted, 2026-11-06. Amends [ADR 0098](0098-mobile-review-bar.md)
(expanding, the expanded view, the buttons on a decided card, and tips).
Written for iOS first; Android follows the same decisions.

## Problem

On an iPhone 17 Pro the maintainer found the review bar's full view hard
to open and cluttered:

- It opens on a flick, not a drag. The grabber reads the gesture only when
  the finger lifts, its hit area is 14 pt high, and a downward flick on
  the compact bar ends review.
- The mentions are cut at a fixed width, so a name can fall off the end.
  They show the alias once replaced and raw Markdown (`**And:**`), with
  nothing highlighted.
- "Keep all 3 / Replace all 3" repeat Keep and Replace with the "All 3"
  scope chip.
- A decided card offers "Keep instead" beside a dead, greyed-out
  "Replaced".
- Four TipKit tips appear as a card that pushes the text down or as
  popovers over the bar. One covers the progress line and ×, Replace looks
  disabled behind it, and closing one brings up the next.

## Decision

### Dragging

- The bar follows the finger from the grabber, the progress line and the
  card, and from the mentions when they are scrolled to the top. Drags are
  measured in screen points, since the bar grows under the finger.
- It settles at **compact** (about 165 pt) or **full** (the mentions' own
  height, at most about 60% of the screen), whichever the fling points
  nearer to. Past full it rubber-bands.
- Dragging down on the compact bar springs back. Only × ends review.
- Opening it scrolls the text so the active name stays visible above the
  bar. A tap on the grabber still toggles it, and VoiceOver's action
  is "Show mentions" or "Hide mentions".

### The full view

- One row per mention of the identity, in document order:
  - the heading the mention sits under ("1. The work");
  - up to about 44 characters either side of the name from its own
    paragraph, cut at words with "…", with Markdown markers dropped;
  - the name in its **original wording**, bold on its category's
    highlight.
- A row's tap scrolls the text there and puts the bar on it. The active
  row is tinted.
- Each row ends in a round **state mark** that is also its button:
  - a dashed ring while proposed: a tap replaces this mention only;
  - an accent disc with a crossed eye once replaced: a tap keeps it;
  - an eye once kept: a tap replaces it.
  The bar stays where it is, and Undo takes each tap back. The card's
  header shows the same mark.
- Keep all and Replace all go. With scope All the bottom buttons say so:
  "Keep all 3", "Replace all 3".

### A decided card

- The buttons are **Undo · Keep instead · Next →** (or "Replace instead"
  once kept), with Next filled.
- "Instead" changes the card in its scope and stays on it.
- Next goes to the next undecided name, or reads "Done" and ends review
  when none is left.

### Hints instead of tips

- No popovers and no tip cards. At most one quiet line per document,
  inside the bar under the progress line, with ×:
  - "Swipe the card for the next name" from the second card;
  - "Long-press any name for more" after three decisions.
- A hint is learned when its gesture is used or × is tapped, and never
  shows again.
- The grabber hops once, on the third card, to show it pulls. The card's
  one-time nudge stays. Reduce Motion skips both.
- The Undo tip goes: the "Replaced · Undo" toast teaches it.
- Settings › Review › Reset tips brings them back at once. UI tests turn
  hints off.

### All reviewed

- Once every name is decided, a line above Share, Copy and Paste says
  "All 15 reviewed · 12 replaced · 3 kept".

## Invariants

- Every gesture still has a visible control and a VoiceOver action
  (ADR 0098). The state marks are buttons labelled with the state.
- Snippets are built from the current Markdown at core source offsets;
  only the name is swapped for its original wording, and nothing is sent
  anywhere.

## Consequences

- TipKit leaves the app; the hints are `ReviewHint` in
  `apps/ios/mdoc/Document/ReviewTips.swift`, keyed in user defaults.
- `MentionSnippet` builds the rows. `DocumentViewModel` gains
  `decideMention` (one mention, the bar stays), `changeCurrent`
  ("instead") and `goToNextUndecided` (Next).

References: [ios-sidebar-grill-2026-11-06](../design/ios-sidebar-grill-2026-11-06.md)
(round 1, Q1 to Q3 and the All reviewed extra in Q7).
