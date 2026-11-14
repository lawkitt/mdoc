# iOS: rc.11's Adjust Words, Identities drawer and Done fixes

Status: Accepted, 2026-11-14. Amends
[ADR 0115](0115-ios-drawer-close-fix-words-in-place-identities-header.md)
(Adjust Words in place, the Identities header) and
[ADR 0101](0101-mobile-review-bar-polish.md) (Done at the end of review).

## Problem

From the maintainer's use of TestFlight rc.11 (build 936, commit 757234ee)
on the iPhone 17 Pro:

1. **Adjust Words' first frames.** Choosing Adjust Words on a name draws
   the alias and the original words over each other for a moment
   ("PHONE_1" and "+7(995)8805144" in one cell), then settles.
2. **Adjust Words in a table cell, tapped away.** A tap elsewhere in the
   document while a cell's name is adjusted clears the cell's tinted
   selection but leaves Adjust Words' bar up: the words are no longer
   shown and Done or Cancel act on nothing visible. Outside a table the
   same tap cancels Adjust Words, which is right.
3. **Done on a document already reviewed.** With every name decided, the
   maintainer opened a card again and tapped Done: "All 19 reviewed ·
   Undo" showed, though nothing had changed; its Undo would undo the last
   real decision, made earlier.
4. **Identities opens without moving.** The top-right button shows the
   Identities drawer at once, while the sidebar's button slides it in.
   The sidebar's button wraps the change in `withAnimation(Motion.move)`
   (`RootView`); `showIdentities` sets `identitiesDrawerOpen` bare.
5. **Select and Cancel's first frames.** In Identities, Select and Cancel
   draw the old and new header and rows over each other for a moment
   (title and Cancel, "Select names" and Select All; each row's name and
   alias twice, offset), then settle.

Problems 1 and 5 look the same: an implicit animation cross-fades text
whose layout changes, so both layouts show at once.

## Decision

- **No cross-fade of changed text.** Entering and leaving Adjust Words,
  and entering and leaving Select, swap their text in one frame; whatever
  moves (bars, the header's buttons, row insets) moves, but no text is
  drawn twice. Reduce Motion: as now.
- **Adjust Words in a table cell ends as it does elsewhere.** A tap outside
  the adjusted words, in a cell or in the body, cancels Adjust Words
  (as Cancel), and the cell shows its alias again. A tap or drag on the
  handles and the selection keeps adjusting.
- **Done on a reviewed document closes the bar quietly.** When the bar
  opens on a document with nothing left to decide, Done closes it with a
  light haptic and no toast. "All N reviewed · Undo" shows only when this
  visit of the bar decided or changed something, and its Undo undoes that.
- **Both drawers slide.** The Identities button, the review bar's
  Identities and every other way in or out animate with the same
  `Motion.move` as the sidebar.

Rejected:

- **A plain "All 19 reviewed" note on Done, without Undo:** it tells the
  user what they knew.
- **Hiding Done on a reviewed document:** the card would differ from every
  other decided card; Done is how the bar closes.
- **Keeping a cell's Adjust Words open after a tap away:** the words
  aren't shown any more, so there is nothing to adjust.

## Consequences

- Slices, one PR each, after this docs PR (ROADMAP iOS item 27): **J**
  Adjust Words (problems 1, 2: `MarkdownTextView+Words.swift`,
  `DocumentViewModel+Words.swift`, the table cells); **K** Identities and
  Done (problems 3, 4, 5: `DocumentViewModel.swift` `showIdentities`,
  `IdentitiesView+Select.swift`, `IdentitiesView.swift`,
  `DocumentViewModel+Review.swift`). J and K touch different files.
- Each fix gets a UI test where one can see it (the cell's tap away, Done
  without a toast); the first-frame fixes are checked frame by frame on the
  iPhone 17 Pro or a Simulator recording.
- TestFlight rc.12 after both merge.
