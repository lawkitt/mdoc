# Mobile UI guide

The current look-and-feel rules for the iOS and Android apps
([ADR 0099](adr/0099-mobile-ui-system.md)). ADRs keep the reasons; this
page keeps the rules and points at the code. Read it before UI work, and
change it in the same PR as the UI.

## How to change the UI

1. Start from an issue labelled `ui`: what's wrong, and what good looks
   like. A screenshot of the [sample document](#sample-document) is safe to
   attach; never attach a screenshot of a real document.
2. Change the [tokens](#tokens) or the [components](#components) first;
   a screen only composes them. A screen with a literal spacing or color is
   a bug.
3. Look at the change in Xcode previews. Every component and screen state
   has a `#Preview` fed by `PreviewFixtures` (light, dark, AX5, iPad).
4. Run the snapshot tests. An intended change re-records its references:
   `TEST_RUNNER_MDOC_RECORD_SNAPSHOTS=1` on `xcodebuild test
   -only-testing:mdocTests/ComponentSnapshotTests` (and the screen
   snapshot classes), then commit the new PNGs under
   `apps/ios/mdocTests/Snapshots/`.
5. Refresh the survey with `just ios-survey docs/evidence/ios-app/current`
   (the iPhone 17 Pro by default) and replace the files in `current/`. The
   PR description carries before and after images; dated folders don't go
   in the tree.
6. Update this guide when a rule changes.

A grill is only for a pattern this guide doesn't cover yet; the guide then
gains it.

## Tokens

iOS: [`apps/ios/mdoc/Design/Tokens.swift`](../apps/ios/mdoc/Design/Tokens.swift).
Android: `apps/android/.../ui/theme` under the same names (to come).

| Token | Values | Use |
|---|---|---|
| `Space` | 4, 8, 12, 16, 24 (`xxs`…`l`) | Padding and gaps; nothing off the 4 pt grid |
| `Radius` | chip (capsule), control 12, card 20, tile 8, tag 4, screen 44 | Continuous corners; screen for the sliding screen beside a drawer |
| `Size` | target 44, bar button 44, chip 30, tile 30 | Every big button is 44 pt high (bar button); hit targets never under 44 pt |
| `Palette` | accent, on accent, swipe accent, surface, background, chip fill, separator; to review (orange), ready (green), restored (blue) | Only named colors; system colors adapt to dark mode and Increase Contrast |
| `Typography` | title, body, caption, chip, alias (monospaced), progress | Dynamic Type text styles only |
| `Motion` | snappy spring 0.25 s; fade 0.2 s under Reduce Motion | `Motion.move(reduced:)` picks one |

Category colors and symbols live in
[`Shared/UI/Palette.swift`](../apps/ios/Shared/UI/Palette.swift) (the
same hues as desktop's highlights), with the tint behind an alias
(`aliasFill`, the document's for a replaced name) and the symbol in its
color for a system menu's row (`menuIcon`). A case's color marks the case
only, never a state.

## Components

iOS: [`apps/ios/mdoc/Design/`](../apps/ios/mdoc/Design/).

| Component | Rule |
|---|---|
| `Chip` | Anything tappable that isn't a button is a filled capsule. A chip that opens a menu ends in a chevron; plain text never acts. Neutral fill, or tinted by category or state. At least 44 pt of target. |
| `AliasTag` | An alias as the text draws a replaced name: monospaced, the text's color on its category's tint, one line, cut in the middle when too long for its row (VoiceOver hears it whole). For an alias in a list or a sheet (the Same as… sheet). A system menu cannot draw it: there the row's icon is `MenuIcon` (the category's symbol in its color) and the alias is `unbrokenAlias`, so the menu cannot hyphenate it: whole up to 12 characters, longer broken only after a separator. |
| `card()` | A raised surface: the review card, empty-state tiles. |
| `StatusPill` | State as an icon and words in the state's color. |
| `SideDrawer` | A panel under the screen that the screen slides aside to show, following the finger; the sliver dims and closes it on a tap. Open, a drag moves it only from the sliver or the panel's inner edge (20 pt, `DrawerDrag`); over the panel, the content owns the drag. While a drawer inside its screen shows, its own edge leaves the swipe to that drawer. |
| `IconTile` | Settings: white symbol on a colored square. Start and document rows: tinted fill, colored symbol. |
| `Toast` / `.toast($message)` | A short message after an action, with at most one action (Undo). Announced to VoiceOver; gone after about 4 s, or 6 s with an action, where a ring shows the seconds left. |
| `StateMark` / `StateButtonLabel` | A mention's state as a round mark: dashed ring proposed, accent disc replaced, eye kept. As a button it has a 44 pt target and the state as its label. |
| `ReviewHintLine` | A hint line: symbol, words, ×. The review bar's, and Identities' one. |
| `EditToolbar` / `ToolbarKey` | Edit mode's bar above the keyboard (ADR 0109): a glass capsule of icon buttons (undo to outdent) that scrolls sideways when they don't fit, then Aa and hide keyboard as glass circles that always show; every button is a 44 pt square target that grows with the text. A button that is on is an ink circle (VoiceOver: selected); one that can't apply is secondary and disabled. A link chip (glass capsule) sits above it while the caret is in a link. The host never becomes first responder, and ends 8 pt above the keyboard (above the home indicator with a hardware keyboard), inside the safe area at a landscape phone's sides. The text keeps room below it for as much of the screen as the toolbar covers plus 12 pt, so the caret's line stays above the toolbar with every keyboard (ADR 0110); on an empty line the caret is a body line tall. |
| `TableMenu` | The table controls (ADR 0110): with the caret in a drawn table's cell, Row below, Column right and a ⋯ button take the place of the list, task, indent and outdent buttons in the `EditToolbar` capsule, which scrolls to its end so ⋯ shows whole (and back to Undo on leaving the cell). ⋯ is a system menu: Row above, Column left, Delete row, Delete column, Align column (a submenu, the column's alignment checked), then Delete table (destructive). A control that cannot apply there (a row above or deleted in the header, the last column deleted) is off; after a new column the grid scrolls to the caret's cell. Nothing is drawn in the document. |
| `FormattingKeyboard` / `FormattingKey` | Aa's keys in place of the keyboard, grouped Turn into, Format, Insert: a symbol over its name on a keyboard-coloured key, the selected block in ink, scrolling at large text sizes. It is the same in a table's cell, where Turn into, Rule and Table are off. Aa brings the system keyboard back. |
| `Haptics` | `selection` per card, `decision` on Replace, Keep and Same as, `edge` at the end of a list, `success` on All reviewed, `warning` on undo. Off when Settings › Review › Haptics is off. |

## Screens

- **Theme:** Settings › Appearance › Theme (Automatic, Light, Dark), stored
  in `Preferences` (`Shared/UI/Preferences.swift`), the App Group's one store
  for every preference (theme, document font, Show Markdown, haptics, the
  hints and the setup card); the app
  sets it on every window and the Restore extension reads it.
- **Accent:** ink, `#1c1c1e` light and `#f2f2f7` dark, in the asset
  catalog's `AccentColor` (`Palette.accent`, `UIColor.ink`), like the app
  icon. Words and symbols on an accent fill use `Palette.onAccent`, white in
  light mode and black in dark mode; a selected filter is
  `Chip(…, filled: true)`. Swipe actions draw their words white in both
  modes, so their accent is `Palette.swipeAccent`, a dark grey in dark mode.
  `AccentContrastTests` holds each pair to 4.5:1.
- **Big buttons** are Liquid Glass, 44 pt high
  ([`Shared/UI/ButtonStyles.swift`](../apps/ios/Shared/UI/ButtonStyles.swift)):
  the primary, `.primaryAction()`, is the system's prominent glass tinted
  ink with on-accent words (the system's keeps white words, which vanish
  on the near-white ink of dark mode); every other big button,
  `.secondaryAction()`, is clear glass with ink words, and Undo and
  Settings are clear glass circles (`secondaryAction(circle: true)`). A
  `Menu` or a toolbar item takes only the system's styles, so "+ New",
  Save to document and Documents' Scan use `primaryGlass()` /
  `secondaryGlass()`, a Menu with `bigButtonLabel(prominent:)` on its
  label. Primary words are semibold. Disabled buttons are clear
  glass with words in the secondary colour. At
  most one primary per view, a drawer or an iPad column counting as its
  own view: on the Documents page Scan is the one, so the setup card's
  Download there is secondary. Screens use these two styles only, no
  literal material or colour; the system's glass adapts to dark mode,
  Increase Contrast and Reduce Transparency.
- **Text buttons** ("Select", "Not now", "Try a sample", Settings' Download
  rows, the toast's Undo) are the primary text colour at medium weight
  (`.textAction()`), not the accent; links in the document are the accent,
  underlined.
- **The name under review** keeps its category's fill and gets an ink
  outline, black in light mode and white in dark mode (`mdocOutline`, drawn
  by `MarkdownLayoutFragment`, in table cells too).
- **Lists** (`BlockLayout`, drawn by `MarkdownLayoutFragment`): bullets,
  numbers and task boxes start at the paragraph's left edge; their text
  starts one column in, 24 pt at the 17 pt body and scaled with the text,
  and each nested level starts at its parent's text. An ordered list whose
  widest number (or written marker) does not fit widens its whole list's
  column. Bullets and numbers are `.label`: numbers at body size, the
  bullet at 1.2× centred in the column's first 16 pt (the variant chosen
  in ADR 0110's list look variants); task boxes are secondary. In edit mode the caret's line shows its source marker (`-`,
  `12.`, `- [ ]`) dimmed in that column, the spare room kerned after it, so
  the text does not move; a marker wider than the column (a task's) has
  its inner spaces narrowed, then its size reduced, to fit (ADR 0110).
- **System controls:** switches stay the system green; menu pickers take
  the ink tint; the segmented Theme control is the system's.
- **Document font:** System or Serif (New York) at the same sizes
  (`Preferences.serif`, read by `MarkdownStyler.font`).
- **Settings:** inset grouped with `SettingsLabel` icon tiles; Appearance,
  Review, Models (plain names, details one tap deeper), Privacy, About.
- **Documents** (was Home): `DocumentRow`, a tinted source tile, the title
  on one line, then `StatusPill`, the time and the `CaseTag`. Search from 8
  documents, a case filter from 2 cases, Scan and + in the bottom bar.
- **Start:** the main area with nothing open: the setup card, Continue
  (the newest document with names to review), the Scan, Files and Paste
  tiles, and Try a sample while there are no documents.
- **Cases:** each case with its colour, document count and how many wait
  for review; a tap shows its documents.
- **A document's panes** ([ADR 0111](adr/0111-ios-panes-row.md),
  `PanesRow`): the bar's centre is always the title's menu. With more
  than one pane a segmented row is pinned under the bar: Text, Original
  and Reply at the content's width on iPhone; on iPad the right column's
  choices at their natural width, centred. It stays in edit mode. On
  iPhone's Reply pane the bottom bar is Share, Copy and Paste for the
  reply, in place of Review and Replace N and the review bar; on iPad
  those stay while names wait, the text being in view. Asking for review
  on iPhone (a name in Identities, Review this identity, a Find match)
  selects Text first (`showTextForReview`).

## Sidebar

[ADR 0102](adr/0102-mobile-sidebar.md); code in
[`apps/ios/mdoc/Library/SidebarView.swift`](../apps/ios/mdoc/Library/SidebarView.swift)
and [`Design/SideDrawer.swift`](../apps/ios/mdoc/Design/SideDrawer.swift).

- **Motion:** on iPhone the screen slides right over the sidebar,
  following the finger from ☰ or the left edge (22 pt), leaving a dimmed
  72 pt sliver with `Radius.screen` corners; a tap on it, or a swipe left
  that starts on it or at the panel's inner edge (20 pt), closes it. Reduce Motion fades. On a wide iPad it is the standing first
  column; on a narrow one (portrait, Slide Over, a split) it slides over as
  on iPhone (`DocumentLayout`).
- **Layout:** search (titles and cases) at the top, no title; the Cases
  and Documents rows; *To review* (orange count of names left) and
  *Recent* (20 at most, green dot when ready); a spinner while a job works
  on a document; the open row highlighted with `Palette.chipFill`. ⚙ (a
  clear glass circle) and "+ New" (the sidebar's primary) float at the
  bottom over a fade. Its menu, nearest the button first: New document
  (an empty document in edit mode, named after its first line until
  renamed and gone if left empty: ADR 0109), Paste text, Files, Photos,
  Scan; the Documents page's + has the same.
- **Rows:** `SidebarRowLabel`, one line, 44 pt high; a long press gives
  Rename, Move to case, Share and Delete. Delete's toast offers Undo; the
  document goes when the toast does or mdoc leaves the screen.
- **Keys:** ⌘K search, ⌘N New document, scene commands, so iPad lists them.

## Review bar

[ADR 0098](adr/0098-mobile-review-bar.md), polished by
[ADR 0101](adr/0101-mobile-review-bar-polish.md); code in
[`apps/ios/mdoc/Document/`](../apps/ios/mdoc/Document/) (`ReviewBar`,
`ReviewCard`, `MentionSnippet`, `ReviewState`, `CandidateActions`,
`DocumentViewModel+Review`). The bar's state is one `ReviewState` value
that only the view model changes; views send intents
([ADR 0104](adr/0104-mobile-app-structure.md)).

- **Layout:** grabber, progress line ("3 of 15" and the decided track; ×
  ends review), a hint line now and then, the card with peeking neighbors,
  the mentions when pulled up, then the buttons. Text stops growing at AX2.
- **Dragging:** the bar follows the finger from the grabber, the progress
  line, the card, and the mentions scrolled to the top (in screen points).
  It settles compact or full (the mentions' height, at most about 60% of
  the screen); down on compact springs back. Opening it keeps the active
  name visible above the bar.
- **Card:** category icon and the name (tap to scroll back to it), its
  state mark once decided, the mentions chip when the name repeats (the
  line is the chip's height either way, so the compact bar keeps one
  height); then the category ▾, alias ▾ and scope ▾ chips; a merge hint
  line while the core sees one.
- **Stepping** (`ReviewPager`): the previous, current and next cards sit
  side by side in one strip, the neighbors peeking in at the bar's edges.
  The finger moves the strip; let go past a third of the card or with a
  fling and it settles on the neighbor with one spring, else springs
  back; at either end it gives a little and bumps. Next, Previous, ← →
  and a tap on a peek move the same strip; Reduce Motion cross-fades.
  The text stays where it is when the name is wholly visible between
  the bars above it and the review bar (the view's own insets); otherwise
  it scrolls just far enough to show the name, at the nearer edge with
  `Space.m` to spare, measured where it is laid out, not where TextKit 2
  estimates it. A table scrolls sideways to a name in it. A decision that
  changes the text keeps a visible name under review at the same place
  on screen (ADR 0098, amended 2026-11-11).
- **Mentions (full):** one row each, in document order: the heading above
  it, the words of its own paragraph cut at words, and the original name
  bold on its category's highlight. A tap on the row goes there; a tap on
  its state mark changes that mention only, and the bar stays.
- **Buttons:** Undo (a clear glass circle), Keep and Replace, the primary
  ("Keep all 3", "Replace all 3" with scope All), all 44 pt high. A
  decided card: Undo, Keep instead (or Replace instead), and Next → as
  the primary ("Done" when nothing is left).
- **Moving on:** a decision steps to the next undecided candidate; the
  last ends review with "All N reviewed · Undo", and a line above Share
  says "All 15 reviewed · 12 replaced · 3 kept". Candidate ids are source
  ranges that move, so the bar keeps its place by position
  (`ReviewOrder`).
- **Undo:** every core step is also registered with the window's undo
  manager, so shake, the three-finger swipe and ⌘Z share the bar's
  history. Undo moves the bar back, with a haptic and no toast, and puts
  an earlier toast away; VoiceOver hears what was undone. Redo is the
  Undo button's long press, ⇧⌘Z and a VoiceOver action, and its toast
  offers Undo ([ADR 0106](adr/0106-mobile-identities-gestures.md)).
- **Menus:** the long press on a name, the card's buttons and the keys
  take their actions from `actions(for:)`, so they agree. The long
  press's header says the name and its alias, the alias on its category's
  tint. The chips' menus show the outcome before the tap ("Organization →
  ORG_3"), each row's icon in its category's color; Same as… lists hints
  first, then the same category, then the rest, and a long list becomes a
  searchable sheet.
- **Tips** (`ReviewHint`): no popovers or cards. One line per document at
  most, inside the bar: "Swipe the card for the next name" from the
  second card, "Long-press any name for more" after three decisions; gone
  for good once the gesture is used or × tapped. The grabber hops once on
  the third card. Off in UI tests; Settings › Review › Reset tips.
- **Keys:** Return Replace, K Keep, ← → step, ⌘. end (Esc too where iOS
  passes it to the app), ⌘Z and ⇧⌘Z.

## Identities

[ADR 0103](adr/0103-mobile-identities-drawer.md), with gestures from
[ADR 0106](adr/0106-mobile-identities-gestures.md); code in
[`IdentitiesView`](../apps/ios/mdoc/Document/IdentitiesView.swift),
`IdentitiesView+Rows`, `IdentitiesView+Gestures`,
`IdentitiesView+DragMerge`, `IdentityDrag`, `IdentitiesPanel` and
`DocumentViewModel+Identities`.

- **Where:** iPhone, a drawer from the right edge or 🪪 (`SideDrawer`,
  trailing); a wide iPad, the standing right column. A tap on a name opens
  the review bar's full view on it.
- **Top:** progress ring, "Identities", "9 names · 4 left", search circle,
  Select, › close; then Replace N, full width in clear glass (a bulk
  shortcut, not the drawer's loudest thing). Outside Select, the hint line
  "In Select, drag ≡ onto a name to make them one" (`ReviewHint.dragMerge`)
  goes here while Looks like has a card or two names share a category,
  gone for good after the first drag-merge or Same as, or ×.
- **Then:** Looks like cards (Same as, Not the same), category chips, and
  the names by what is left: To review, Replaced, Kept (folded).
- **Rows:** category icon, the name (wrapped at words), the alias, "3×"
  and its other spellings in grey, and a round `StateButtonLabel`: replace, then Replaced ⇄
  Kept, with Undo in the toast. A decided row's mark turns to its new
  mark first (a cross-fade under Reduce Motion), then the row moves to
  its section.
- **Swipes:** right is Replace, a full swipe too; left shows Same as… and
  Keep, never as a full swipe, since Keep sends the original to the AI.
  In the drawer the rows own sideways swipes.
- **Drag-to-merge** (`IdentitiesView+DragMerge`, `IdentityDrag`): in
  Select each row has a ≡ handle at its trailing edge, where the state
  mark is outside Select. Dragging it lifts a copy of the row that follows
  the finger up and down; the row itself dims. The row under the finger
  (from the rows' reported frames, never the dragged row nor one under a
  pinned section header) gets the ink outline and "Same as PERSON_1" in
  its alias line, on the copy too; within 56 pt of the list's top or
  bottom (the selection bar's top) the list scrolls. Letting
  go on a row is the core's link, as desktop's drag: the target keeps its
  alias, the restore wording is the core's fullest form, another category
  takes the target's, one undo step, a haptic and "PERSON_3 is now
  PERSON_1 · Undo"; the selection keeps its other names. Anywhere else the
  copy slides back. The drag is the app's own gesture on the list:
  SwiftUI's drag and drop never delivers a drop inside the List it started
  in. VoiceOver: the handle is "Drag to make one with another name", with
  Make one with…, which opens Same as…. Mention drags and drop-to-split
  are desktop's only.
- **Select:** tick rows, then Replace, Keep or Same as (into the name
  mentioned most); one Undo takes the selection back. Rows in Select have
  no swipes and no long-press menu: a tap ticks, the ≡ handle drags.

## Edit mode

[ADR 0109](adr/0109-mobile-markdown-editor.md), polished by
[ADR 0110](adr/0110-mobile-editor-polish.md); code in `DocumentScreen`,
`MarkdownTextView+Editing` and `DocumentViewModel+Editing`.

- **One text view** serves review, edit mode and the Text/Original
  switch: the pencil, Done and the switch keep the line at the top of the
  view, the caret and a selection. On iPhone the text view stays alive,
  hidden, under the Original.
- **Entering** (the pencil, ⌘E): with a selection the keyboard comes up
  on it; with none nothing moves, names dim and the keyboard waits for a
  tap in the text. A new, empty document opens with the keyboard up.
- **The bar:** ☰, the title, Done as a
  checkmark in its own tinted glass capsule (`.glassProminent`, VoiceOver
  "Done editing", ⌘E), and ⋯. In review: the pencil, Identities and ⋯ in
  one capsule. The title truncates at its tail in the room between ☰ and
  the trailing buttons, centred.
- **The Original while editing** (iPhone): the switch stays; Original
  hides the keyboard and keeps edit mode on, and Text brings the keyboard
  back at the same caret.
- **A selection in a table's cell** is made by a double tap or a long
  press on a word and stretched by its handles, as in the text: the
  highlight follows the finger with the loupe on the handle, the edit
  menu shows when the finger lifts, and typing or the toolbar acts on it.
  It stays in its cell: a handle dragged past the cell stops at the end
  of that line, or at the cell's start or end, and Select All selects the
  cell's text ([ADR 0110](adr/0110-mobile-editor-polish.md)).
- **Identities are hidden:** on iPhone no button and no right-edge swipe,
  and an open drawer closes; on iPad the inspector or the right column
  gives way to the Original and comes back on Done if it was showing.

## Gestures

Every gesture also has a visible control and a VoiceOver action; no action
exists only as a gesture ([ADR 0098](adr/0098-mobile-review-bar.md)).

| Where | Gesture | Does |
|---|---|---|
| Review card | Swipe sideways, or tap a peek | Previous or next candidate |
| Review bar grabber, progress line, card | Drag up or down | Full or compact, following the finger; × ends review |
| A name in the text | Long press | Its menu (Replace, Keep, Same as…, Change category) |
| A table's cell, editing | Double tap or long press; drag a handle | Selects the word; stretches the selection inside the cell |
| Identities row | Swipe right or left, or tap its round button | Replace (a full swipe too); Same as… and Keep (no full swipe); the button replaces, then switches Replaced and Kept |
| Identities row, in Select | Drag ≡ onto another row | Same as: the two names become one |
| Anywhere | Shake, three-finger swipe | Undo and redo |
| Left edge (iPhone) | Swipe right | The sidebar (with Identities open, it closes Identities); tap the sliver, or swipe left from it or the panel's inner edge, to close |
| Right edge (iPhone, a document open, not editing) | Swipe left | Identities; tap the sliver, or swipe right from it or the panel's inner edge, to close |

No swipes on the document text.

## Wording

- Short, plain verbs: Replace, Keep, Same as…, New alias, Undo.
- Toasts name what happened: "12 replaced", "Same as Maya Bennett",
  "PERSON_3 is now PERSON_1".
- Plain names for models ("Name finder"); the technical name sits one tap
  deeper.
- US spelling.

## Accessibility

- Every screen works at every Dynamic Type size; bars stop growing at AX2
  and use the large content viewer.
- Every action has a VoiceOver label and, where it's a gesture, an action.
- Motion respects Reduce Motion (`Motion.move(reduced:)`), and colors come
  from the palette, so Increase Contrast and dark mode follow.

## Sample document

[`Shared/Core/SampleDocument.swift`](../apps/ios/Shared/Core/SampleDocument.swift):
a short services agreement with made-up names, in English and Russian,
chosen by the phone's language. It is "Try a sample" in the app and the
fixture for previews and tests, so screenshots of it are safe to share.
The sample is editable; Try a sample again opens it with its text as it
came, asking "Restore the sample?" first when it was edited (ADR 0109).

## Screenshots

- `docs/evidence/ios-app/current/` and `docs/evidence/android-app/current/`
  hold the latest survey, replaced by each UI PR.
- `just ios-survey <folder> [device]` runs `SurveyUITests` (iPhone 17 Pro
  by default); `just android-survey` on the Windows PC for Android.
- Earlier dated sets are in git history: the
  [2026-10-31 iOS survey](https://github.com/lawkitt/mdoc/tree/1e7096412fda2d27ec5a274e5b53ba00c3b101f1/docs/evidence/ios-app/2026-10-31-ui-survey)
  and later sets beside it.
