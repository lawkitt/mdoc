# iOS UI grill, 2026-11-14

Interview record for the bugs the maintainer found in TestFlight rc.10
(build 930, commit 9c92a4dd) on the iPhone 17 Pro, after
[ADR 0114](../adr/0114-ios-menu-swipes-and-name-words.md) shipped: the
drawer's corners, closing a drawer by a drag, Fix the words on the review
card, Fix the words itself, and (added in round 1) the Identities
controls. Decisions go into an ADR amending ADR 0114 once the tree is
closed; this file keeps the findings, the answers and the open branches.

## Findings (main e4e8ca3e)

**1. The drawer's corners.** Screenshot: the sidebar open, the screen's
sliver at the right; the corners beside the sliver look wrong.

**2. Closing a drawer.** Open, only a drag that starts on the sliver or
within 20 pt of the panel's inner edge moves the drawer
(`DrawerDrag.movesOpenDrawer`, ADR 0106). Identities rows own both sideways
swipes: right is Replace (full swipe allowed), left shows Keep and Same as…
(no full swipe). Identities opens from the right, so its close drag (to
the right) is Replace's swipe. Sidebar rows have no swipe actions.

**3. Fix the words on the review card.** The chip row is a sideways
`ScrollView` with `.scrollClipDisabled()`: category, alias and scope
already fill a 402 pt card for "Tax identifier", so the Fix the words chip
sits off-screen or draws over the neighbouring card in the carousel. Any
long row overlaps the same way ("Belongs to…").

**4. Fix the words.** While a name's words are fixed, the draft text still
carries the name's `mdoc-candidate://` link, and
`MarkdownTextView.Coordinator.textView(_:menuConfigurationFor:defaultMenu:)`
returns UIKit's default menu when `wordsFix != nil`: a long press or a
handle drag on the name lifts the link and shows its raw URL with Open,
Copy and Share (a leak past ADR 0085's export invariant). Hide «…» shows
twice: an edit-menu callout over the selection and the bottom bar's
button.

## Round 1 (2026-11-14): all recommendations accepted

1. **The sidebar closes by a leftward drag anywhere over it**, following
   the finger as opening does; vertical scrolls, the search field and long
   presses are never taken; the sliver and the edge keep working.
2. **Identities: rows give up the right swipe.** A left swipe shows
   Replace | Same as… | Keep, Replace outermost, so a full left swipe
   replaces (it has Undo; Keep still needs a tap). A rightward drag
   anywhere over Identities closes it, rows included. Amends ADR 0106.
3. **Fix the words on the card moves into the alias chip's menu**, beside
   Rename… (Rename… / Fix the words / Same as… / New alias). The chip row
   is clipped at the card's edge with a short fade, so no chip draws over a
   neighbouring card.
4. **Added:** a branch on making the Identities controls more elegant and
   better organized (the header, the category filter chips, the list's
   section headers), and bugs found by trying rc.10 on the 17 Pro.

## Findings for round 2 (iPhone 17 Pro Simulator, iOS 26.5, main e4e8ca3e)

Reproduced with an invented bilingual contract (a three-column table with
identifiers in its cells).

- **1. Corners:** the screen's shadow is a straight gradient strip
  (`DrawerEdgeShadow`) along its edge, full height. Beside the screen's
  rounded corners it runs on to the top and bottom: a square grey band
  outside the curve.
- **4. Fix the words:** a drag that starts exactly on a handle's knob moves
  it (loupe, the bar follows: Hide «Taxpayer ID…»). A press on the name,
  or on a handle over it, starts the candidate link's long press instead,
  shows `mdoc-candidate://candidate?id=2` with Open, Copy and Share, and
  undid the handle's change. The name keeps its candidate box and
  underline under the selection.
- **Identities header** (the maintainer's screenshot): "12 names · all
  decided" wraps beside 🔍 and the Select capsule; at all decided the ring
  is a full accent (black) circle that reads as an empty radio button; the
  category chips are the list's first row, overflow the panel's edge with
  no fade and scroll away; the first section header fades under the list's
  top edge effect.

## Round 2 (2026-11-14): all recommendations accepted

4. **The drawer's shadow follows the screen's curve**: a layer shadow on a
   fixed rounded path, set once per layout and moved only by transform
   (nothing changes per frame), checked on the iPhone 12 Pro hitch test
   (#862's pending `testADrawerDragsHitches`). If the 12 Pro drops frames,
   no shadow: the dimmed sliver and the drawn corners separate the screen.
5. **Fix the words is fixed in place.** While a name's words are fixed, the
   text shows the draft without candidate links or name decoration: plain
   text with one tinted selection, whose handles behave as any selection's
   (loupe, word snapping, a long press selects again). The edit-menu
   callout goes; the bottom bar alone holds the hint, Cancel and Hide «…»,
   following the handles. **In every state a candidate link never shows
   the system's link menu** (no menu instead of UIKit's default).
6. **In a table cell**, while a name's words are fixed, the table doesn't
   scroll sideways, and it first scrolls so the cell is wholly on screen.
7. **The Identities header, reorganized:** row 1 the title, the summary on
   its own line under it ("8 of 12 decided", "12 names · all decided ✓"),
   and the Select capsule alone at the top right; row 2 a search field,
   always shown, and one filter capsule whose menu lists All and each
   category with its icon and count (in place of the chip row). Both rows
   stay pinned above the list; the first section header isn't faded.
8. **The ring goes**: the summary's words carry the progress.

## Findings for round 3 (rc.10 on the iPhone 17 Pro, the maintainer's documents)

Driven by a throwaway XCUITest bundle with no host app, so the TestFlight
install and its data stayed as they were.

- **Identities header:** while names are left it has a third row, a
  full-width "Replace 32" capsule; "52 names · 32 left" wraps there too.
- **Pinned section headers** ("To review 32") are near invisible (tertiary
  text on a translucent band), with a faded ghost of the row beneath; the
  same in Select.
- **The document menu** lists no Copy for AI or Share while names are
  left: the section is hidden until the document is ready
  (`DocumentScreen`, `phase == .ready`).
- **Home:** VoiceOver reads the "73 left" icon as "Error"
  (`exclamationmark.circle`'s own label).
- **Tables:** a table whose header cells are all empty draws a blank grey
  header bar (the contract's converted table).
- **The review card:** the left neighbour's Fix the words chip shows at
  the card's left edge too (item 3).
- **Recognition flaws** seen on the way ("Агента" as a person, "INN" and
  "People's Republic of China" as organizations, "may" in the sample):
  for the recognition hub, not this grill.

## Round 3 (2026-11-14): accepted, with the gate changed

9. **Replace N leaves the Identities header** for a floating glass
   "Replace 32" capsule at the panel's bottom, where Select's bar shows in
   Select; the header keeps two rows.
10. **Copy for AI and Share are always in the document menu**, disabled
    with the reason under the title ("32 names left to decide") while the
    document isn't ready, as Find names again is.
11. **Small fixes:** Home's "N left" reads "N names left to review" to
    VoiceOver; Identities' pinned section headers get an opaque background,
    a secondary-colour count and no ghost row; a table header row whose
    cells are all empty isn't drawn.
12. **Delivery:** a docs PR first (ADR 0115, this record, ROADMAP iOS
    item 26); then three slices, one PR each, with the Codex review and
    hosted CI, enqueued on the maintainer's word:
    **G** the drawer (shadow on the curve, close drag from anywhere for
    both drawers, Identities rows on a left swipe), built in this session;
    **H** Fix the words (in-place redesign, the link-menu safety net, the
    table lock, the alias-menu item, the clipped chip row) and **I** the
    Identities header and the small fixes, each by a builder in its own
    worktree and Simulator (two builders on this Mac).
13. **rc.11's gate: the iPhone 17 Pro only**, no 12 Pro run this time;
    tagged by hand as rc.8 and rc.10 were. Q4's measured 12 Pro hitch check
    therefore waits for a later build; the curved shadow is checked by eye
    on the 17 Pro, and goes (Q4's fallback) if a drag stutters.

The tree is closed:
[ADR 0115](../adr/0115-ios-drawer-close-fix-words-in-place-identities-header.md),
ROADMAP iOS item 26.
