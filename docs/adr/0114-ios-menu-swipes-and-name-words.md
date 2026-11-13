# iOS: one document menu, document-wide drawer swipes, fixing a name's words

Status: Accepted, 2026-11-14. Amends
[ADR 0098](0098-mobile-review-bar.md) (a name's long-press menu gains Fix
the words), [ADR 0101](0101-mobile-review-bar-polish.md) (TipKit returns
as the hints' engine), [ADR 0103](0103-mobile-identities-drawer.md) and
[ADR 0106](0106-mobile-identities-gestures.md) (the Identities header and
where a drawer swipe starts), [ADR 0110](0110-mobile-editor-polish.md)
and [ADR 0111](0111-ios-panes-row.md) (the bar's trailing items).
Record: [ios-ui-grill-2026-11-13](../design/ios-ui-grill-2026-11-13.md).

## Problem

From the maintainer's use of TestFlight rc.9 on the iPhone 17 Pro:

- The document's commands sit in two menus: the title menu and a ⋯ that
  holds only Find in document and Show Markdown.
- The sidebar and Identities open only from a 22 pt strip at the screen's
  edge, and the drag lags and flickers. Each frame sets SwiftUI state that
  re-runs the drawer, the screen's corner mask jumps on the first frame,
  its shadow radius changes every frame, and the panel is built only once
  the drag starts.
- The Identities header needs a hint line ("In Select, drag ≡ onto a name
  to make them one") to explain Select, beside a › close button.
- A selection gets a bare one-item Hide name menu over a grey highlight,
  and a found name's words can't be corrected: a long press inside a name
  lifts the whole name and opens its menu, so "ФИО Тагиев Тебриз Фарадж
  оглы" can't be narrowed to the name.
- Entering edit mode in a large document with a large table crashed on the
  phone (report pending), and in the Simulator jumped to the document's
  end.

## Decision

### One document menu

- **⋯ goes.** The title menu holds, in groups: Find in document, Find
  names again | Show Markdown (a toggle) | Copy for AI, Share | Rename…,
  Move to case › | Delete.
- The review capsule is the pencil and 🪪; edit mode's bar is ☰, the title
  and ✓. Share and Copy for AI stay in the menu: the review bar replaces
  the action bar while reviewing.

### Drawer swipes

- **A sideways swipe anywhere on the document** opens a drawer: right the
  sidebar, left Identities, once the finger has moved more than 10 pt and
  mostly sideways. A vertical scroll is never taken.
- **Exceptions:** content that can scroll sideways keeps every sideways
  swipe that starts on it, even at its edge: a table wider than the
  screen, and an Original page zoomed past its width. A table that fits and
  a page at fit width let the swipe through. The review bar and its card
  carousel keep their swipe; selection handles and long presses are never
  taken.
- **Edit mode:** the edges only; Identities stays hidden there (ADR 0110).
- The same on iPad wherever the drawers are used.
- **The drag runs in UIKit:** one pan recognizer that declines by the
  rules above, moving layers by transform; SwiftUI hears only open or shut
  at the end. No per-frame shadow or mask changes; the panel stays built
  while closed.
- This replaces docs/mobile-ui.md's "No swipes on the document text".

### The Identities header

- **No › button and no hint line.** The sliver, a swipe, 🪪 and VoiceOver's
  escape close the drawer.
- **Outside Select:** ring, title, summary, 🔍 and a "Select" glass capsule
  with ☑︎.
- **In Select**, as in Mail and Photos: Cancel | "3 selected" | Select
  All. Rows get a leading tick circle, the ≡ handle stays, the bottom bar
  keeps Replace, Keep and Same as.
- **The first time Select opens** with two names or more, a demo plays on
  the real rows: a ghost of the first row lifts on its ≡ handle and settles
  on the row below, which shows "Same as PERSON_1", under the caption "Tick
  names to act on them, or drag ≡ onto another name to make them one".
  Once, about 2 s; any tap ends it. Reduce Motion: a still picture with the
  caption. VoiceOver reads the caption. Learned for good by the first
  drag-merge or Same as, or by dismissing it.
- **Identities rows** get a tinted state glyph in place of a filled white
  circle on every row.

### Hints on TipKit, drawn by mdoc

- **TipKit decides when; mdoc draws how.** The Select demo and the three
  existing hints (card swipe, long press, drag-merge) become TipKit tips
  read through `shouldDisplay` / `statusUpdates`; none uses `TipView` or
  `.popoverTip`. The existing hints keep their quiet line.
- Tips appear on the thing they explain, cover no control and leave every
  button live, show one at a time (`.tipsDisplayFrequency`), and are
  invalidated by doing the action (TipKit events).
- Settings › Reset tips calls `Tips.resetDatastore()`; UI tests call
  `Tips.hideAllTipsForTesting()`.

### Hide name and Fix the words

- **A selection outside any name:** a tinted highlight; the menu's item
  reads Hide "Тагиев Тебриз…" (truncated), beside a Hide as › submenu of
  the categories with the core's guess first. Handles snap to whole words;
  leading and trailing punctuation is trimmed, as the line's newline is.
  The review card opens on the new mark as before.
- **Fix the words:** a new item in a name's long-press menu and on the
  review card. The name gets selection handles in the text (loupe, word
  snapping); a Hide «…» callout confirms; Undo takes it back. A replaced
  name shows its original wording while its words are fixed.
- **Scope:** the new wording replaces the old wording everywhere it
  appears and keeps the identity and its alias, in one undo step: a core
  action shared with desktop. New words that are exactly another
  identity's wording join that identity.
- **Limits:** the handles stay in one paragraph or table cell and never
  cross Markdown formatting (the core's rule for marked names); a handle
  stops where a drag would break it. Nothing, or only spaces and
  punctuation, can't be hidden.
- **The review card's title** wraps to two lines instead of a cut in the
  middle.

### Edit mode in a large document

- Entering edit mode keeps the place in a document with tables, and the
  phone's crash is fixed from its report, with a test on a large table.

Rejected:

- **A long press inside a name starts a plain selection:** breaks the
  long-press menu (ADR 0098) and makes tap-to-review harder.
- **Patching the SwiftUI drag:** SwiftUI's `DragGesture` can't decline a
  touch because a table under it could scroll, which the table rule needs.
- **TipKit's own `TipView` and popovers:** what ADR 0101 took out (cards
  pushing the text, popovers hiding the bar's controls, tips chaining).
- **Keeping `ReviewHint` and adding the demo to it:** the display rules,
  frequency and testing switch would stay hand-made.
- **Fixing form labels in spans here:** a detection flaw like the others,
  [#859](https://github.com/lawkitt/mdoc/issues/859), fixed with them.

## Consequences

- Slices, one PR each: **A** the crash and the edit-mode jump (first);
  **B** the menu, the Identities header and the Select demo with the
  TipKit move; **C** the drawer engine and document-wide swipes, with a
  measured check on the iPhone 12 Pro (no dropped frames while dragging);
  **D** the Hide name callout, Fix the words and its core action; **F** the
  card's title and the rows' glyphs. B and C run in parallel after A; then
  D and F. TestFlight rc.10 after all merge.
- `SideDrawer` is rewritten around a UIKit pan; `DocumentScreen` loses
  `moreMenu`; `IdentitiesView`'s header and `ReviewTips.swift` change;
  `MarkdownTextView`'s edit menu and the candidate menu gain items;
  `mdoc-ffi` gains a re-span review action.
- docs/mobile-ui.md's Gestures table, Identities and Edit mode sections
  change with the slices.
- Desktop gets a To-grill line for Fix the words on the same core action.
