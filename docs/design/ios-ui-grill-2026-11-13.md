# iOS UI grill, 2026-11-13

Interview record for five iOS UI items the maintainer raised after using
TestFlight build rc.9 (899) on the iPhone 17 Pro: the ⋯ menu, drawer swipes,
the Identities header, fixing a name's words, and a crash entering edit
mode in a large document. Decisions go into an ADR once the tree is
closed; this file keeps the findings, the answers and the open branches.

## Findings (iPhone 17 Pro Simulator, iOS 26.5, main c6a59305)

Reproduced by hand in the Simulator, seeded (`-mdoc-ui-testing`,
`MDOC_SEED_FILE`) with two invented fixtures: a Russian form like the
maintainer's (labels, tables, a name after "ФИО") and a bilingual contract
with a 140-row, two-column table and 60 paragraphs after it.

**1. Two menus.** The title menu holds Find names again, Copy for AI,
Rename…, Move to case, Share, Delete; ⋯ holds only Find in document and
Show Markdown. In edit mode ⋯ is the bar's only item beside Done.
`DocumentScreen.titleMenu` / `moreMenu`. Share and Copy for AI also sit in
the action bar, but the action bar gives way to the review bar while
reviewing, so the menu is their only place then.

**2. Drawer swipes.** `SideDrawer` opens only from a 22 pt edge strip
(`edgeWidth`); docs/mobile-ui.md says "No swipes on the document text".
Each frame of a drag sets SwiftUI `@State` (`drag`), which re-runs the
drawer's body: the content's `.mask` corner radius jumps from 0 to the
screen radius on the first point, the `.shadow` radius changes every frame
(an offscreen pass over the whole screen), and the panel is inserted
(`if showsPanel`) only when the drag starts, so Identities or the sidebar
builds its view in the first frames. Likely causes of the lag and flicker;
not measured on the phone. Tables in the text are horizontal scroll views
(`MarkdownTableView`), the review card has its own sideways swipe.

**3. Identities header.** Ring, "Identities", "12 names · all decided",
search circle, the text button Select (Done in Select), a › circle that
closes the drawer, then the hint line "In Select, drag ≡ onto a name to make
them one" (`ReviewHint.dragMerge`). Select gives ticks, a bottom bar
(Replace, Keep, Same as) and ≡ drag handles.

**4. Hide name.** A selection outside a found name shows the system edit
menu with one item, Hide name; the selection is grey, not tinted, while
it waits. The core marks the selection (`ReviewAction::Mark` →
`add_manual`), and a hand-marked span wins any overlap with a detected one
(`mdoc-pii`, "Manual additions win discovery overlaps"). But **a long press
inside a found name lifts the whole name and opens its menu** (Replace,
Keep, Just this one, Same as…, Change category), so the words inside it
can't be selected: a wrong span such as "ФИО Тагиев Тебриз Фарадж оглы"
can't be narrowed. Nothing on desktop or mobile adjusts a found name's
words. The review card's title truncates in the middle ("ФИО
Петр…Сергеевич").

**5. Crash entering edit mode.** The phone's report
(`mdoc-2026-11-12-232601.ips`) sits in Universal Clipboard's folder, which
macOS privacy keeps from the agent; not read yet. In the Simulator (iOS
26.5, not 27) the big-table fixture did not crash, but **entering edit mode
at row 17 of the table jumped to the end of the document** (the place is
lost).

**Noticed on the way**

- The detector's person span took the form label "ФИО" (the maintainer's
  screenshot); labels such as ФИО, Name:, Ф.И.О. could be left out by
  `mdoc-detect` (desktop gains too).
- Identities rows each carry a filled white state circle; in dark mode the
  list reads as a column of white buttons.

## Round 1 (2026-11-13): all recommendations accepted

1. **One document menu.** ⋯ goes; the title menu holds, in groups: Find in
   document, Find names again | Show Markdown (toggle) | Copy for AI, Share
   | Rename…, Move to case › | Delete. The review capsule is the pencil and
   🪪; edit mode's bar is ☰, the title and ✓. Share and Copy for AI stay in
   the menu because the review bar replaces the action bar while reviewing.
2. **Drawer swipes start anywhere on the document**: right opens the
   sidebar, left Identities, once the finger has moved >10 pt and mostly
   sideways; a vertical scroll is never taken. Exceptions: a table wider
   than the screen keeps every sideways swipe that starts on it (even at
   its edge; a table that fits does not block); the review bar and its
   carousel keep theirs; selection handles and long presses are never
   taken. Edit mode: the edges only (Identities stays hidden there). The
   same on iPad wherever the drawers are used. Replaces docs/mobile-ui.md's
   "No swipes on the document text".
3. **The drag runs in UIKit**: one pan recognizer that declines by Q2's
   rules, moving layers by transform; SwiftUI hears only open or shut at
   the end; no per-frame shadow or mask changes; the panel stays built
   while closed. Done includes a measured check on the 12 Pro (no dropped
   frames while dragging).
4. **Identities header.** No › button (the sliver, a swipe, 🪪 and
   VoiceOver's escape close it) and no hint line. Outside Select: ring,
   title, summary, 🔍 and a "Select" glass capsule with ☑︎. In Select, as in
   Mail and Photos: Cancel | "3 selected" | Select All; rows get a leading
   tick circle, the ≡ handle stays, the bottom bar keeps Replace, Keep,
   Same as. The maintainer adds: **a first-time explainer when Select is
   first opened** (shape in round 2).
5. **Fix the words**: a new item in a name's long-press menu and on the
   review card; the name gets selection handles in the text (loupe, word
   snapping), a Hide «…» callout confirms, Undo takes it back. The new
   wording replaces the old wording everywhere it appears and keeps the
   identity and its alias. A replaced name shows its original wording
   while its words are fixed. Core: a re-span action or mark + drop in one
   undo step (to check).
6. **Hide name callout** for a selection outside any name: a tinted
   selection; the item reads Hide "Тагиев Тебриз…" (truncated); a Hide as ›
   submenu of the categories, the core's guess first; handles snap to whole
   words and leading or trailing punctuation is trimmed. The review card
   opens on the new mark as now.
7. **Also in:** (a) the review card's title wraps to two lines instead of
   a middle cut; (b) the detector leaves form labels (ФИО, Ф.И.О., Name:,
   Фамилия Имя Отчество) out of a person span: its own core PR with
   detection tests; (c) Identities rows get a tinted state glyph, not a
   filled white circle; (d) entering edit mode keeps the place in a table
   document. The crash (item 5) is fixed once the phone's report is read.

## Round 2 (2026-11-14)

- **Q7(b) deferred.** Form labels in a person span become a detection flaw,
  [#859](https://github.com/lawkitt/mdoc/issues/859) (flaw, hub:windows),
  fixed with the other recognition flaws; no slice here.
- **Fix the words, edge cases (accepted).** The handles stay inside one
  paragraph or table cell and never cross Markdown formatting (the core's
  rule for marked names); a handle stops where a drag would break it. New
  words that are exactly another identity's wording join that identity,
  with Undo. Nothing, or only spaces and punctuation, can't be hidden.
- **Desktop (accepted).** The same core action fixes a wording everywhere;
  a desktop To-grill line now, desktop's own UI later.
- **The Original pane (accepted).** A page zoomed so it scrolls sideways
  keeps sideways swipes; at fit width the drawer swipe passes, as tables.
- **Delivery (accepted, less E).** A docs PR first (ADR, this record,
  ROADMAP iOS items); then A (crash + edit-mode jump) first, B (menu,
  Identities header, Select explainer) and C (drawer engine) in parallel,
  then D (Hide name callout, Fix the words, core action) and F (card title,
  row glyphs). At most three builders, separate Simulators; Codex reviews
  each PR; rc.10 after all merge, through the usual gate.
- **The Select explainer: TipKit?** The maintainer asks why not Apple's
  TipKit. Context: TipKit was used and taken out by
  [ADR 0101](../adr/0101-mobile-review-bar-polish.md) (cards pushing the
  text down, popovers over the bar covering × and making Replace look
  disabled, one tip chaining into the next). Round 3.

## Round 3 (2026-11-14)

- **Hints: TipKit as the engine, mdoc's own look (accepted).** The Select
  demo and the three existing hints become TipKit tips read through
  `shouldDisplay` / `statusUpdates`, drawn by mdoc (no `TipView`, no
  popovers): on the thing they explain, covering no control, one at a time
  (mdoc holds the others back; TipKit's display frequency stays
  immediate), invalidated by doing the action. Reset tips resets each
  tip's eligibility (`resetEligibility()`); UI tests
  `Tips.hideAllTipsForTesting()`.
  Amends ADR 0101's "TipKit leaves the app".

The tree is closed: [ADR 0114](../adr/0114-ios-menu-swipes-and-name-words.md),
ROADMAP iOS item 25, a desktop To-grill line, CONTEXT terms Document menu and
Fix the words. Open fact: the phone's crash report (slice A).
