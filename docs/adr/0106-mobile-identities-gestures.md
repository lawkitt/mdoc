# Mobile Identities gestures and drag-to-merge

Status: Accepted, 2026-11-08. Amends [ADR 0103](0103-mobile-identities-drawer.md)
(rows), [ADR 0098](0098-mobile-review-bar.md) (swipes, drag-to-merge on
iPad) and [ADR 0101](0101-mobile-review-bar-polish.md) (toasts). Record:
[gestures grill](../design/ios-identities-gestures-grill-2026-11-08.md).

## Problem

In the Identities drawer a long left swipe keeps a name without the user
choosing to, the drawer's own close drag takes rightward swipes meant for
a row (Replace, or putting revealed actions away), and nothing shows that
a name can be dragged onto another. After Undo a Redo toast with a timer
adds noise.

## Decision

- **Rows own horizontal swipes** in the open drawer. The drawer closes
  from ›, a tap on the dimmed sliver, a swipe that starts on the sliver,
  or a swipe from the drawer's inner edge (about 20 pt).
- **Swipe right: Replace**, full swipe allowed, with a haptic and the Undo
  toast. **Swipe left: Same as… and Keep**, no full swipe: Keep sends the
  original to the AI, so it is always a tap.
- A decided row's state mark animates to its new mark before the row
  moves to its new section.
- **Undo shows no toast**: the change and a haptic are the feedback. Redo
  stays on the Undo button's long press, ⇧⌘Z and VoiceOver; a Redo shows
  the Undo toast.
- **Drag a name onto another to make them one**, on iPhone and iPad, from
  a ≡ handle: in Select each row shows one at its trailing edge, in the
  state mark's place, and dragging it lifts a copy of the row that follows
  the finger. Rows don't drag freely: SwiftUI's List never delivers a drop
  of a drag that starts inside it, so the drag is the app's own gesture on
  the handle. Desktop's rules (ADR 0025): the dragged name joins the
  target, which keeps its alias; the restore wording follows the core's
  rule, the fullest form of the names (#388), not the target's. One undo
  step, no confirmation; another category takes the target's; the
  selection keeps its other names. While dragging, the row under the
  finger shows an ink outline and "Same as ALIAS"; the list scrolls near
  its edges; on drop, a haptic and the Undo toast "ALIAS is now TARGET ·
  Undo"; let go anywhere else and the copy slides back. VoiceOver has
  Make one with… on the handle, which opens Same as…. Mention drags and
  drop-to-split stay desktop-only.
- **One hint line** in the drawer, "In Select, drag ≡ onto a name to make
  them one", outside Select when Looks like has a suggestion or two names
  share a category; gone for good after the first drag-merge or Same as.

## Consequences

- Closing the drawer by swiping over the list no longer works; the sliver,
  the edge and › do.
- Keep needs one more tap from a swipe than before.
- Drag-to-merge needs Select first, where rows neither swipe nor show
  the long-press menu; outside Select a long press on a row is its menu.
