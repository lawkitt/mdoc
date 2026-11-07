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
| `Radius` | chip (capsule), control 12, card 20, tile 8, screen 44 | Continuous corners; screen for the sliding screen beside a drawer |
| `Size` | target 44, bar button 40, chip 30, tile 30 | Hit targets never under 44 pt |
| `Palette` | accent, surface, background, chip fill, separator; to review (orange), ready (green), restored (blue) | Only named colors; system colors adapt to dark mode and Increase Contrast |
| `Typography` | title, body, caption, chip, alias (monospaced), progress | Dynamic Type text styles only |
| `Motion` | snappy spring 0.25 s; fade 0.2 s under Reduce Motion | `Motion.move(reduced:)` picks one |

Category colors and symbols live in
[`Shared/UI/Palette.swift`](../apps/ios/Shared/UI/Palette.swift) (the
same hues as desktop's highlights). A case's color marks the case only,
never a state.

## Components

iOS: [`apps/ios/mdoc/Design/`](../apps/ios/mdoc/Design/).

| Component | Rule |
|---|---|
| `Chip` | Anything tappable that isn't a button is a filled capsule. A chip that opens a menu ends in a chevron; plain text never acts. Neutral fill, or tinted by category or state. At least 44 pt of target. |
| `card()` | A raised surface: the review card, empty-state tiles. |
| `StatusPill` | State as an icon and words in the state's color. |
| `SideDrawer` | A panel under the screen that the screen slides aside to show, following the finger; the sliver dims and closes it on a tap. |
| `IconTile` | Settings: white symbol on a colored square. Home: tinted fill, colored symbol. |
| `Toast` / `.toast($message)` | A short message after an action, with at most one action (Undo, Redo). Announced to VoiceOver; gone after about 4 s. |
| `StateMark` / `StateButtonLabel` | A mention's state as a round mark: dashed ring proposed, accent disc replaced, eye kept. As a button it has a 44 pt target and the state as its label. |
| `ReviewHintLine` | The review's one hint line: symbol, words, ×. |
| `Haptics` | `selection` per card, `decision` on Replace, Keep and Same as, `edge` at the end of a list, `success` on All reviewed, `warning` on undo. Off when Settings › Review › Haptics is off. |

## Screens

- **Theme:** Settings › Appearance › Theme (Automatic, Light, Dark), stored
  in the App Group (`AppTheme`, `Shared/UI/MarkdownDisplay.swift`); the app
  sets it on every window and the Restore extension reads it.
- **Accent:** teal green, `#336b5c` light and `#82b9a7` dark, in the asset
  catalog's `AccentColor`.
- **Document font:** System or Serif (New York) at the same sizes
  (`MarkdownDisplay.serif`, read by `MarkdownStyler.font`).
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

## Sidebar

[ADR 0102](adr/0102-mobile-sidebar.md); code in
[`apps/ios/mdoc/Home/SidebarView.swift`](../apps/ios/mdoc/Home/SidebarView.swift)
and [`Design/SideDrawer.swift`](../apps/ios/mdoc/Design/SideDrawer.swift).

- **Motion:** on iPhone the screen slides right over the sidebar,
  following the finger from ☰ or the left edge (22 pt), leaving a dimmed
  72 pt sliver with `Radius.screen` corners; a tap on it or a swipe left
  closes it. Reduce Motion fades. On a wide iPad it is the standing first
  column.
- **Layout:** search (titles and cases) at the top, no title; the Cases
  and Documents rows; *To review* (orange count of names left) and
  *Recent* (20 at most, green dot when ready); a spinner while a job works
  on a document; the open row highlighted with `Palette.chipFill`. ⚙ and
  "+ New" float at the bottom over a fade.
- **Rows:** `SidebarRowLabel`, one line, 44 pt high; a long press gives
  Rename, Move to case, Share and Delete.
- **Keys:** ⌘K search, ⌘N Start.

## Review bar

[ADR 0098](adr/0098-mobile-review-bar.md), polished by
[ADR 0101](adr/0101-mobile-review-bar-polish.md); code in
[`apps/ios/mdoc/Document/`](../apps/ios/mdoc/Document/) (`ReviewBar`,
`ReviewCard`, `MentionSnippet`, `ReviewState`, `DocumentViewModel+Review`).

- **Layout:** grabber, progress line ("3 of 15" and the decided track; ×
  ends review), a hint line now and then, the card with peeking neighbors,
  the mentions when pulled up, then the buttons. Text stops growing at AX2.
- **Dragging:** the bar follows the finger from the grabber, the progress
  line, the card, and the mentions scrolled to the top (in screen points).
  It settles compact or full (the mentions' height, at most about 60% of
  the screen); down on compact springs back. Opening it keeps the active
  name visible above the bar.
- **Card:** category icon and the name (tap to scroll back to it), its
  state mark once decided, the mentions chip when the name repeats; then
  the category ▾, alias ▾ and scope ▾ chips; a merge hint line while the
  core sees one.
- **Mentions (full):** one row each, in document order: the heading above
  it, the words of its own paragraph cut at words, and the original name
  bold on its category's highlight. A tap on the row goes there; a tap on
  its state mark changes that mention only, and the bar stays.
- **Buttons:** Undo, Keep and Replace ("Keep all 3", "Replace all 3" with
  scope All). A decided card: Undo, Keep instead (or Replace instead),
  and Next → filled ("Done" when nothing is left).
- **Moving on:** a decision steps to the next undecided candidate; the
  last ends review with "All N reviewed · Undo", and a line above Share
  says "All 15 reviewed · 12 replaced · 3 kept". Candidate ids are source
  ranges that move, so the bar keeps its place by position
  (`ReviewOrder`).
- **Undo:** every core step is also registered with the window's undo
  manager, so shake, the three-finger swipe and ⌘Z share the bar's
  history; Undo moves the bar back and the toast offers Redo.
- **Menus:** the long press on a name and the chips' menus show the
  outcome before the tap ("Organization → ORG_3"); Same as… lists hints
  first, then the same category, then the rest, and a long list becomes a
  searchable sheet.
- **Tips** (`ReviewHint`): no popovers or cards. One line per document at
  most, inside the bar: "Swipe the card for the next name" from the
  second card, "Long-press any name for more" after three decisions; gone
  for good once the gesture is used or × tapped. The grabber hops once on
  the third card. Off in UI tests; Settings › Review › Reset tips.
- **Keys:** Return Replace, K Keep, ← → step, ⌘. end (Esc too where iOS
  passes it to the app), ⌘Z and ⇧⌘Z.

## Gestures

Every gesture also has a visible control and a VoiceOver action; no action
exists only as a gesture ([ADR 0098](adr/0098-mobile-review-bar.md)).

| Where | Gesture | Does |
|---|---|---|
| Review card | Swipe sideways, or tap a peek | Previous or next candidate |
| Review bar grabber, progress line, card | Drag up or down | Full or compact, following the finger; × ends review |
| A name in the text | Long press | Its menu (Replace, Keep, Same as…, Change category) |
| Identities row | Swipe right or left | Replace; Keep and Same as… |
| Anywhere | Shake, three-finger swipe | Undo and redo |
| Left edge (iPhone) | Swipe right | The sidebar; swipe left or tap the sliver to close |

No swipes on the document text.

## Wording

- Short, plain verbs: Replace, Keep, Same as…, New alias, Undo.
- Toasts name what happened: "12 replaced", "Same as Maya Bennett",
  "Undid Replace · Riverside Holdings".
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

## Screenshots

- `docs/evidence/ios-app/current/` and `docs/evidence/android-app/current/`
  hold the latest survey, replaced by each UI PR.
- `just ios-survey <folder> [device]` runs `SurveyUITests` (iPhone 17 Pro
  by default); `just android-survey` on the Windows PC for Android.
- Earlier dated sets are in git history: the
  [2026-10-31 iOS survey](https://github.com/lawkitt/mdoc/tree/1e7096412fda2d27ec5a274e5b53ba00c3b101f1/docs/evidence/ios-app/2026-10-31-ui-survey)
  and later sets beside it.
