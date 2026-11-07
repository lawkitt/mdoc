# Mobile sidebar

Status: Accepted, 2026-11-06. Amends [ADR 0099](0099-mobile-ui-system.md)
(Home) and [ADR 0087](0087-ios-ui-polish.md) (iPad's columns). Written for
iOS first; Android follows the same decisions.

## Problem

On a phone, mdoc's documents live on Home, and every document is a push
away from it: going to another document means Back, find it, open it. Its
long press lists Rename and Move to case in ADR 0099, but neither exists.
In the middle of a review the way to another document is three taps and a
lost place. The maintainer uses the Claude iOS app daily and wants mdoc's
navigation to feel the same: a sidebar that slides over the open
document.

## Decision

### Navigation

- **The sidebar is the hub.** The main area shows a document, a section's
  page (Cases, Documents), or **Start** when nothing is open. Documents show
  ☰ where Back was; there is no navigation stack between Home and a
  document.
- **Start** holds what Home's empty state held: the setup card while
  models are missing, a "Continue · <title> · 3 left" card for the newest
  document with names to review, the Scan, Files and Paste tiles, and Try a
  sample while there are no documents.

### The sidebar

- **Motion, as in Claude.** On iPhone the sidebar lies under the screen.
  ☰ or a swipe from the left edge slides the screen right, following the
  finger. A dimmed sliver about 72 pt wide stays, with rounded corners.
  A tap on the sliver or a swipe left closes it. Reduce Motion fades
  instead.
- **Top:** a search field over titles and case names, full width, with no
  title above it. While searching, results replace the lists, with Cancel.
- **Section rows:** *Cases* (a page: each case with its colour, its
  document count and how many wait for review) and *Documents* (every
  document, with search and the case filter, as Home had).
- **Lists:** *To review*, each row ending in an orange count of names left,
  then *Recent* (up to 20), a green dot on documents ready to share. A
  document a job is working on shows a spinner. The open row has a filled
  highlight.
- **Bottom:** ⚙ (Settings) in a circle and a filled "+ New" capsule with
  Paste text, Files, Photos and Scan, floating over a fade.
- **A row's long press:** Rename, Move to case, Share, Delete.
- **Review:** opening the sidebar mid-review keeps the bar's place; the bar
  slides away with the document.
- **Keys:** ⌘K opens the sidebar's search, ⌘N goes to Start.

### Rename and Move to case

(ROADMAP iOS item 9.)

- **Rename** renames the document, from its sidebar row, the Cases page or
  the menu on the document's title. A case with only this document that is
  named after it takes the new name.
- **Move to case** follows desktop's rule (ADR 0041). The document leaves
  its case and joins the other, and `plan_join` lines up the names: names
  the case knows take its aliases, clashing aliases take new numbers, and
  the case's other names are proposed. The case it leaves stays, with its
  mapping, even with no documents left, as on desktop: replies already
  sent restore with it, and deleting a case is a confirmed step of its own
  (ADR 0041). The menu lists the five most recent cases, then New case…
  and All cases…, and a toast offers Undo, which moves the document back
  and takes the names it brought out of the other case again.

### iPad

- A wide window keeps the same sidebar as its standing first column. A
  narrow one (or Slide Over) slides it over, as on iPhone.

## Invariants

- Every gesture has a visible control: ☰ opens the sidebar and the sliver
  closes it, and VoiceOver's Escape closes it too. While it is open, the
  screen under it is hidden from VoiceOver.
- The sidebar never shows a document's text or names, only titles and case
  names, as Home did.

## Consequences

- `HomeView` becomes the Documents page; the add flows move to
  `IntakeFlows`, shared by Start, the sidebar and Documents.
- `SideDrawer` (`apps/ios/mdoc/Design/`) is the drawer component;
  Identities can use it from the right edge.
- UI tests reach documents and Settings through the sidebar
  (`MdocUITest.revealSidebar`).

References: [ios-sidebar-grill-2026-11-06](../design/ios-sidebar-grill-2026-11-06.md)
(round 1, Q4 to Q7).
