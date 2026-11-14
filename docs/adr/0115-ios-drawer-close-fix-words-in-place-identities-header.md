# iOS: drawers close as they open, Fix the words in place, a calmer Identities header

Status: Accepted, 2026-11-14. Amends
[ADR 0114](0114-ios-menu-swipes-and-name-words.md) (the drawer's shadow,
Fix the words, the review card's Fix the words chip, the document menu),
[ADR 0106](0106-mobile-identities-gestures.md) (rows' swipes and where an
open drawer's drag starts) and [ADR 0103](0103-mobile-identities-drawer.md)
(the Identities header and Replace N).
Record: [ios-ui-grill-2026-11-14](../design/ios-ui-grill-2026-11-14.md).

## Problem

From the maintainer's use of TestFlight rc.10 (build 930) on the iPhone 17
Pro, and a look at it through a device probe:

- The drawer's shadow is a straight strip along the screen's edge: beside
  the screen's rounded corners it runs on as a square grey band.
- An open drawer closes only by a drag from the sliver or the panel's
  inner edge, while it opens by a swipe anywhere on the document.
  Identities rows own both sideways swipes, and the close drag (rightward)
  is Replace's swipe.
- The review card's Fix the words chip doesn't fit its row: it sits off
  the screen or draws over the neighbouring card.
- Fix the words: the name keeps its candidate link while its words are
  fixed, so a press on it or on a handle over it lifts the link and shows
  the system's link menu, with the raw `mdoc-candidate://` URL and Open,
  Copy and Share (past [ADR 0085](0085-mobile-apps.md)'s export
  invariant); Hide «…» shows twice; the name's box stays under the
  selection.
- The Identities header takes three rows (ring and title with 🔍 and Select;
  Replace N; category chips that overflow and scroll away), its summary
  wraps, a full ring reads as an empty radio button, and pinned section
  headers are near invisible.
- The document menu hides Copy for AI and Share until every name is
  decided; Home's "N left" icon is read as "Error"; an all-empty table
  header row draws a blank bar.

## Decision

### The drawer

- **The shadow follows the screen's curve:** a layer shadow on a fixed
  rounded path, set once per layout and moved only by transform; nothing
  changes per frame. If a drag stutters, no shadow at all: the dimmed
  sliver and the drawn corners separate the screen.
- **An open drawer closes by a drag from anywhere over it**, following the
  finger as opening does, besides the sliver, its tap and the edge:
  leftward over the sidebar, rightward over Identities. Vertical scrolls,
  text fields and long presses are never taken.
- **Identities rows give up the right swipe.** A left swipe shows Replace |
  Same as… | Keep, Replace outermost, so a full left swipe replaces (it has
  Undo; Keep still needs a tap). Select's rows don't swipe, as before.

### Fix the words, in place

- While a name's words are fixed, **the text shows the draft without
  candidate links or name decoration**: plain text with one tinted
  selection whose handles behave as any selection's (loupe, word snapping,
  a long press selects again).
- **One Hide «…»:** the edit-menu callout goes; the bottom bar holds the
  hint, Cancel and Hide «…», following the handles.
- **In a table cell** the table doesn't scroll sideways while the words are
  fixed, and first scrolls so the cell is wholly on screen.
- **A candidate link never shows the system's link menu**, in any state.
- **On the review card** Fix the words moves into the alias chip's menu:
  Rename…, Fix the words, Same as…, New alias. The chip row is clipped at
  the card's edge with a short fade, so no chip draws over a neighbour.
- The rest of ADR 0114's Fix the words stands: scope, limits, the core
  action, Undo.

### The Identities header

- **Row 1:** the title, the summary on its own line under it ("8 of 12
  decided", "12 names · all decided ✓"), and the Select capsule alone at
  the top right. **The ring goes.**
- **Row 2:** a search field, always shown, and one filter capsule whose
  menu lists All and each category with its icon and count, in place of
  the chip row.
- Both rows stay pinned above the list. Pinned section headers get an
  opaque background and a secondary-colour count.
- **Replace N** floats as a glass capsule at the panel's bottom; Select's
  bar takes its place in Select.

### Small fixes

- **The document menu always lists Copy for AI and Share**, disabled with
  the reason under the title ("32 names left to decide") until the
  document is ready, as Find names again is.
- Home's "N left" reads "N names left to review" to VoiceOver.
- A table header row whose cells are all empty isn't drawn.

Rejected:

- **Keeping both row swipes and closing Identities only off the rows:**
  the drag would work in some places and not others.
- **A wider inner edge only:** still not the document's "from anywhere".
- **An icon button for Fix the words in the card's header, or a menu on
  the card's name:** the header's title would lose room, and tapping the
  name scrolls back to it; the action is rare.
- **A sheet, or word steppers, for Fix the words:** a sheet loses the page
  around the name; steppers are slow for big changes. Steppers may come
  later as VoiceOver actions on the bar.
- **Category chips kept, scrolling with a faded edge:** they never fit a
  phone with more than three categories.

## Consequences

- Slices, one PR each, after a docs PR (this ADR, the record, ROADMAP iOS
  item 26): **G** the drawer (`SideDrawer.swift`,
  `IdentitiesView+Rows.swift`); **H** Fix the words
  (`MarkdownTextView`, `MarkdownTextView+Words.swift`, `FixWordsBar.swift`,
  `ReviewCard.swift`); **I** the Identities header and the small fixes
  (`IdentitiesView.swift`, `DocumentScreen.swift`, the table renderer,
  Home). G and I touch different Identities files.
- TestFlight rc.11 after all three merge, gated on the iPhone 17 Pro only
  and tagged by hand. The measured 12 Pro drag check that ADR 0114 asked
  for waits for a later build.
- docs/mobile-ui.md's Gestures table and Identities section change with
  the slices.
