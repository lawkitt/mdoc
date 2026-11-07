# iOS sidebar and review polish grill, 2026-11-06

The maintainer used the iOS 0.2 build on an iPhone 17 Pro and asked for
three things:

1. The review bar's full view expands on a swipe, not a drag, and looks
   cluttered, with buttons that repeat each other.
2. Tips feel heavy and get in the way.
3. A sidebar like the Claude iOS app's that slides over the open
   document, with Home's Rename and Move to case
   ([ROADMAP](../../ROADMAP.md) iOS 0.2 item 9) folded in.

The maintainer's own screenshots show client data, so they aren't
quoted or committed. Every finding below was reproduced with the
[sample document](../mobile-ui.md#sample-document).

## What the app does today

From the code on `main` at e13935c3 and the sample in the iPhone 17 Pro
Simulator (iOS 26.3):

- **Expanding.** `ReviewBar`'s grabber reads a `DragGesture` only when it
  ends (more than 30 pt up expands, down collapses or ends review), and
  its hit area is 14 pt high. The bar never follows the finger, and a
  short flick on the grabber often does nothing. A tap toggles it.
- **The full view** (`ReviewDetails`) lists the identity's mentions
  under a "Mentions (3)" caption. The snippets are cut at a fixed width
  around the mention, so the name can fall off the end. They show the
  alias once replaced, raw Markdown (`**And:**`), and nothing
  highlighted. Below them, "Keep all 3 / Replace all 3" repeat Keep and
  Replace with the "All 3" scope chip.
- **A decided card** turns the buttons into "Keep instead" and a greyed-out
  "Replaced": one live button and one dead one.
- **Tips** are four TipKit tips. Long-press shows as a card that pushes
  the document down, and the others as popovers over the bar. Swipe's
  popover covers the progress line and ×, and Replace looks disabled while
  it shows. Dismissing one brings up the next ("Pull up for details")
  straight away.
- **Identities** is a sheet. At half height it shows two names under a
  title row, a search field with a stray underline and a segmented
  filter. Each row has three lines (name, alias, "3 mentions") and a
  state pill with words.
- **Home** is the root of a `NavigationSplitView`; documents push onto
  it with a back button. Its long press has Open, Share and Delete only.

## The Claude iOS app

Looked at on the maintainer's iPhone through iPhone Mirroring, without
sending anything:

- A swipe from the left edge, or ☰, slides the whole screen right. The
  sidebar sits underneath; a dimmed sliver of the screen stays at the
  right with rounded corners, and a tap on it closes the sidebar.
- The sidebar has a serif title and a round search button. Below them
  are four section rows (Projects, Code, Artifacts, Scheduled), then
  *Pinned* and *Recents*: one-line rows, each with a trailing status
  (blue unread dot, or a spinner while it runs). The open section has a
  filled highlight.
- At the bottom float an avatar circle, which opens Settings as a full
  sheet, and a white "+ New session" capsule. The list fades under them.
- A section row opens a full page in the main area (Code: device chips,
  then sessions with a status icon and a repo · branch subtitle), with ☰
  where a back button would be.
- Search takes over the top of the sidebar with a full-width field and ×,
  lists recents, and shows keyboard hints (↑↓ move · Return opens).

## Round 1: decisions

All seven recommendations were accepted, with one change (Q5: no title
at the top of the sidebar).

### Q1. Dragging the review bar

- The bar follows the finger from the grabber and the progress line, and
  from the details list when it is scrolled to the top, with a taller
  grab area. It settles at **compact** (about 165 pt) or **full** (about
  60% of the screen), with velocity carrying a fling to the next stop.
- The text scrolls so the active name stays visible above the bar.
- Dragging down on the compact bar springs back. Only × ends review, so
  review can't end by accident. (Amends ADR 0098's "swipe down on the
  compact bar ends review".)

### Q2. The full view

- The card stays as it is.
- Below it, one row per mention: a snippet centred on the name, the
  **original name** in bold and tinted, the nearest heading in front
  ("Parties · …"), whitespace and Markdown markers dropped. A trailing
  state pill (Replaced / Kept) changes that one mention on a tap; a tap
  on the row scrolls the text there.
- "Keep all / Replace all" go. With scope All, the bottom buttons read
  "Keep all 3 / Replace all 3".
- On a decided card the buttons are **Undo · Keep instead · Next →**,
  with Next filled. The dead "Replaced" button goes.

### Q3. Tips

- No popovers or tip cards.
- At most one quiet line per document, inside the bar under the
  progress line, gone as soon as the gesture is used: "Swipe the card
  for the next name" after the second card; "Long-press any name for
  more" later.
- "Pull up for details" becomes one small bounce of the grabber.
- The Undo tip goes: the "Replaced · Undo" toast teaches it.
- Settings › Review › Reset tips stays.

### Q4. Where navigation lives

- The sidebar is the hub, as in Claude. The main area shows a document,
  or a **Start** page when none is open: the setup card, the Scan, Files
  and Paste tiles, Try a sample, and "Continue · <title> · 3 left".
- Documents show ☰ where the back button was. Home's list moves into
  the sidebar.

### Q5. The sidebar

- **No title.** The top row is search only (titles and cases), full
  width as in Claude.
- **Section rows:** *Cases* (a page: each case with its color, document
  count, Rename and Delete) and *Documents* (every document, with case
  and state filters).
- **Lists:** *To review* (a trailing orange count of names left) and
  *Recent* (a green dot for ready, blue for a restored reply). A document
  being scanned or searched shows a spinner.
- **Bottom:** a ⚙ circle for Settings and a "+ New" capsule with Scan,
  Photos, Files and Paste text.
- **Row long press:** Rename, Move to case, Share, Delete.
- **Gestures:** opens from the left edge or ☰, never from a swipe on the
  text; closes with a swipe left or a tap on the dimmed sliver. Review
  keeps its place; the bar slides away with the document.

### Q6. Rename and Move to case

- **Rename** renames the document, from its sidebar row, the Cases page,
  or the menu on the document's title. A case with only this document
  and named after it is renamed with it.
- **Move to case** follows desktop's rule ([ADR 0041](../adr/0041-cases-and-ai-response-restoration.md)):
  the document leaves its case and joins the other. Names the target
  case knows take its aliases, clashing aliases take new numbers, and the
  target's other names are proposed. Replies already sent restore with
  the old case, which keeps its mapping. An emptied case is deleted
  unless it was shared or holds a reply. The menu lists the five most
  recent cases, then New case… and All cases…; a toast says "Moved to
  <case> · Undo".
- The core gains rename document, rename case and move to case.
- **Amended in review (#660):** an emptied case is not deleted. Deleting
  it lost the case an unsaved import was joining, and a case shared
  before the current one, whose replies then no longer restored. The case
  a document leaves stays, as on desktop (`delete_document` already
  keeps one); deleting a case is a confirmed step of its own (ADR 0041),
  for later.

### Q7. iPad, extras and PRs

- **iPad:** a wide window keeps the sidebar as its standing left column;
  a narrow one slides it over, as on iPhone.
- **Extras in this run:** the title menu (Rename, Move to case, Share,
  Copy for AI, Delete); spinners on working documents; an "All reviewed"
  card in the bar (Share, Copy for AI, Paste reply) in place of the plain
  bottom bar; ⌘K search and ⌘N new on a keyboard.
- **Later (ROADMAP):** Home Screen quick actions, pinned cases.
- **PRs:** (1) review bar and tips; (2) core rename and move; (3) the
  sidebar, Start page, Rename and Move. Each refreshes the survey.

## Round 2: Identities

Identities today, on the sample: at half height the sheet fits two names
under a title row, a search field with a stray underline and a four-way
segmented filter. Each row has three lines and a "Replaced/Proposed" pill
with words, grouped by category, so what is left to decide is spread over
the sections. On the maintainer's own document (not quoted) one person
appears twice, once in Latin and once in Cyrillic letters, as two
identities. `mdoc-pii`'s merge hints treat transliteration as no signal on
purpose (`initials_alone_and_transliteration_are_not_signals`), so nothing
suggests they are one person.

All seven recommendations were accepted.

### Q8. Where Identities lives on iPhone

- A **right-edge drawer**, the mirror of the sidebar: the documents on the
  left, this document's names on the right. It slides in from the right
  edge or the 🪪 button, with the same motion (`SideDrawer`, trailing). On
  a wide iPad it stays the standing right column.
- A tap on a name closes it and opens the review bar's full view on that
  identity: the full view is the identity's detail.

### Q9. Order

- By what is left: *To review* first, then *Replaced*, then *Kept*
  (folded). Each row shows its category's icon; a chip row filters by
  category ("Person 2 · Organization 2 · Address 3").

### Q10. Rows

- Two lines at most: the category icon in its colour, the name, and its
  other spellings in grey ("· Ms Bennett"); then the alias and "3×".
- A trailing round state button, like Reminders' checkbox and the full
  view's state marks: a tap replaces a proposed name, then switches
  Replaced and Kept, with an Undo toast. Swipes and the long press stay.

### Q11. Top of the panel

- A summary line ("15 names · 4 to review") with a small progress ring, a
  filled **Replace 4** for the names left, and a search circle that widens
  across the panel, as in Claude. No title row, Done or segmented filter.

### Q12. Merging and bulk

- A **Looks like** card at the top for the core's merge hints ("Maya
  Bennett · Ms Bennett", Same as, ×).
- **Select:** tick rows, then Replace, Keep or Same as (merge them into
  one). Drag a row onto another to merge stays on iPad.

### Q13. Transliterated pairs

- Not in this run: a ROADMAP item for its own grill, since it changes
  recognition on desktop too. Select and Same as fix a pair by hand.

### Q14. Where it ships

- PR 4, after the sidebar, sharing its drawer.

## Ideas from the journey (not decided)

From walking the sample through Start, review, hand-off and restore in
the Simulator, and from the Claude app. Each would become a ROADMAP
"To grill" item if the maintainer wants it.

1. **Long jobs in the Dynamic Island.** Claude shows its running work
   there; mdoc's OCR and name finding (minutes on a long scan) could show
   "Recognizing page 3 of 12" as a Live Activity, beside the system's
   background-task progress.
2. **Peek at the originals.** Press and hold an eye in the toolbar to see
   the original names in place of the aliases, released to hide them: a
   quick check before sharing, without undoing anything.
3. **Paste a reply from Start.** After sharing, Start offers "Got a reply?
   Paste it" for the case shared last, so the restore step is one tap from
   where the user lands.
4. **A case's page.** Tapping a case shows its names (the case-wide
   identities with aliases), its documents and its replies, the phone's
   version of desktop's case page (ADR 0077).
5. **Shortcuts and App Intents.** "Scan into mdoc" and "Restore a reply"
   as Shortcuts actions and Spotlight entries; Home Screen quick actions
   are already a ROADMAP item.
6. **Document thumbnails** on the Documents page: the first page of a PDF
   or scan, so lawyers find a document by sight.
7. **A widget:** "3 documents to review", opening the first.
