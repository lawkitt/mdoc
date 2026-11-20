# Mobile

Rules for the iOS and Android apps. A rule or section marked iOS is iOS
only; Android follows the rest as it reaches each screen. UI and snapshot
tests specify each screen's layout and motion; iOS implementation rules are
in `apps/ios/README.md`.

## Changing the UI

1. Start from an issue labelled `ui`: what's wrong and what good looks like.
   Attach screenshots of the [sample document](#sample-document) only, never
   of a real document.
2. Change the tokens or the components first; a screen only composes them.
3. Check previews and snapshots (iOS: `apps/ios/README.md`, UI changes).
4. Take before and after shots with `just ios-survey <folder>` or `just
   android-survey <folder>` outside the repository, for the PR description;
   survey PNGs are never committed.
5. Change this page in the same PR when a rule changes.
6. The maintainer checks the TestFlight build.

A grill is only for a pattern this page lacks, which this page then gains.

## The core and the app

- Every rule about documents, review, cases, aliases, file names and restore
  lives in `crates/mdoc-ffi` or below; the app formats and lays out what a
  snapshot says.
- No Swift or Kotlin copy of a core rule: a rule found in the app moves into
  the core.
- The app never derives a review fact from other facts; a fact the app needs
  is added to the core's snapshot.
- View-model and snapshot tests run the real core over temporary storage
  with a fixed case key; there is no fake library or document session.
- Test fakes and the fixed case key are never compiled into the app or the
  extension.
- Each piece of state has one owner: one navigation model for what is open
  and presented, presented from one place; a document's model for intent, with how it shows derived
  from the window size in one place.
- Review selection is one value that only the view model changes; views send
  intents, and tests drive it without a view.
- Each document has its own undo history; switching documents never undoes
  another document's step.
- A review action is one core call returning the new snapshot; unchanged
  Markdown is never re-parsed.
- The platform wrapper keeps no state beside the core's handle.
- Lists of documents come from the core's summaries, without opening each
  document.
- A review tap returns its snapshot at once and saves off the main thread
  once taps pause (300 ms), because a save on the tap took longer than a
  frame.
- A pending save runs at once on leaving the foreground, before Copy for AI,
  Share, an import, Move to case or leaving the document.
- No fixed delay in app code except for a named platform bug, a missing
  event or a deliberate on-screen duration, each marked `// timing:` in a
  form `scripts/dev/fixed-delays.sh` accepts.
- UI tests wait for elements, never for time.
- Every bug fix has a test that fails without it.
- A fresh reviewer agent reviews every iOS PR, whether or not Codex is
  available.

## Device and models

- No rules-only tier, because a scan that finds emails but no names gives a
  false sense of safety.
- Below the floor the app imports, recognizes text and marks names by
  selection; where Find names would be, and in Settings, it says why and
  that selecting marks a name.
- The floor is 4 GB of RAM (`DETECTOR_FLOOR_BYTES`), or a timed run past
  10 minutes per 10,000 characters; it guards memory, not speed, because
  owners of older phones would rather wait.
- An offered model peaks under 2 GiB and cold-loads within 10 s on each
  device class.
- A scan needs about 1.3 GB free to start (`DETECTOR_FREE_BYTES`), because
  the system would end the app partway.
- A download on cellular asks first on any cellular interface, whatever the
  system's expensive or metered flag says.
- A download held for Wi-Fi says "Waiting for Wi-Fi" with Use cellular.
- iOS: each model downloads on its own, with Download all beside; Android
  still downloads its models together.
- A document's setup card offers the model its job needs and resumes the
  job once installed.
- A phone scan has no deadline; it shows time left and stops after 60 s
  without progress.
- Heat pauses a scan but never withdraws the detector.
- One long job runs at a time, off the main thread, with progress and
  cooperative cancellation.
- Nothing starts in the background on its own; only a job the user started
  continues there.
- The + menu reads New document, Paste, Files, Photos, Scan, the camera
  furthest from the button.
- Nothing outside the app (widgets, Live Activities, Spotlight,
  notifications, quick actions) shows a document's or case's title, because
  titles hold client names.
- `just ios-device-test <udid>` runs before every TestFlight build, because
  the Simulator never runs the real background task.
- `just android-device-test` runs by hand before every Android release
  after the first.

## Tokens

In `apps/ios/mdoc/Design/Tokens.swift`; Android mirrors them in `ui/theme`.

| Token | Values | Rule |
|---|---|---|
| `Space` | 4, 8, 12, 16, 24 | Nothing off the 4 pt grid |
| `Radius` | capsule, 12, 20, 8, 4, screen 44 | Continuous corners |
| `Size` | target 44, bar button 44 | No hit target under 44 pt |
| `Palette` | named colors; to review orange, ready green, restored blue | No literal color or material |
| `Typography` | Dynamic Type styles | No fixed sizes |
| `Motion` | spring 0.25 s; fade under Reduce Motion | `Motion.move(reduced:)` |

- Category colors live in `Shared/UI/Palette.swift`, desktop's hues.
- A case's color marks the case only, never a state.

## Components

iOS: `apps/ios/mdoc/Design/`.

- `Chip`: anything tappable that isn't a button is a filled capsule; ▾ or a
  chevron opens a menu; plain text never acts.
- `AliasTag`: an alias is monospaced on its category's tint, one line, cut
  in the middle when too long.
- `Toast`: at most one action (Undo).
- `StateMark`: a dashed ring while proposed, an eye.slash on the category's
  tint once replaced, an eye on the chip fill once kept; as a button it has
  a 44 pt target and the state as its label.
- A state mark is never an ink disc, because a column of them reads as
  buttons in dark mode.
- A state mark's glyph is the text color, because category colors fail
  contrast as glyphs in light mode.
- Haptics go through `Haptics`, which Settings › Review › Haptics turns off.

## Look

- Every preference lives in `Preferences`, the App Group store the Restore
  extension reads too.
- The theme is Automatic, Light or Dark, applied at once to every screen and
  sheet.
- The accent is ink (`#1c1c1e` light, `#f2f2f7` dark); words on it use
  `Palette.onAccent`; `AccentContrastTests` holds each pair to 4.5:1.
- Swipe actions use `Palette.swipeAccent`, since their words are white.
- Big buttons are 44 pt high, in two styles only: primary and secondary
  (iOS: `apps/ios/README.md`, Buttons).
- At most one primary per view; a drawer or an iPad column counts as its own
  view.
- Text buttons are the primary text color at medium weight
  (`.textAction()`), not the accent.
- Links in the document are the accent, underlined.
- The name under review keeps its category's fill with an ink outline.
- Every category's underline reaches 3:1 on the dark background; candidate
  and alias fills are stronger in dark than in light.
- No text is hyphenated anywhere.
- A button bar never wraps a label: it stacks, the primary on its own row.
- Bars stop growing at the second accessibility size.

## Rendered Markdown

- The text view's string is the Markdown source, so candidate offsets are
  the core's UTF-8 source offsets and Copy for AI bytes match desktop.
- Outside edit mode, with Show Markdown off, every marker is hidden, in a
  selection too.
- iOS: tables, lists, quotes, alerts, code and rules are drawn by one
  TextKit 2 view's layout fragments, never a web view. Android shows the
  text in a `TextView`.
- What to hide and decorate comes from the core (`mdoc-markdown`, through
  `markdown_styling`); the app maps spans to fonts and colors only.
- Headings and paragraph and list spacing keep desktop's relative sizes,
  scaled by Dynamic Type.
- A table's cells wrap in columns at least about 140 pt wide, scaled with
  Dynamic Type and never narrower than their longest word; the table
  scrolls sideways.
- Candidates in cells look and act as in text.
- A selection stays inside one cell or outside tables.
- A header row whose cells are all empty isn't drawn outside edit mode; the
  text keeps it.
- Bullets are `•` at every depth; ordered items show the number written in
  the text in their depth's style (`1.`, `a.`, `i.`), never a count, because
  a clause number is content.
- Bullets and numbers take the text color.
- List markers start at the paragraph's edge, their text one 24 pt column
  in, scaled with the text; each level starts at its parent's text.
- A task box toggles only through edit mode's Task command.
- The Text pane, the Reply pane and Restore with mdoc render alike, and all
  follow Show Markdown, one setting for the app, off by default.
- Test documents copy the structure of real ones, never their names or
  numbers.

## Navigation

- The sidebar is the hub: no navigation stack between Start, the section
  pages and a document.
- The sidebar and Identities are drawers on iPhone and a narrow iPad,
  standing columns on a wide iPad.
- The sidebar shows titles and case names only, never a document's text or
  names.
- Every way into or out of a drawer animates with `Motion.move`.
- The document's one menu is the title's; there is no ⋯ menu.
- The title menu always lists Copy for AI and Share; until the document is
  ready they are off, with the reason under the title.
- With more than one pane, a pinned row under the bar switches Text,
  Original and Reply (`pane-picker`), because controls that move under the
  finger get mis-tapped.
- The pane row fits three segments in a 320 pt window at every text size.
- On iPhone's Reply pane the bottom bar acts on the reply; asking for review
  from there selects Text first.

## Review

- A docked review bar takes the action bar's place; nothing dims and the
  text stays scrollable and tappable.
- Opening, stepping and scrolling never close the bar; only ×, Done or the
  last decision do.
- Replace and Keep apply on tap in the scope shown; there is no staged
  Apply.
- Scope is All by default, This one on demand; it resets on the next
  identity and covers every chip.
- Stepping goes through every candidate in document order.
- Replace or Keep on a proposed name moves to the next undecided one;
  changing a decided name (Keep instead, Replace instead, a state mark)
  stays on it.
- In the full view a row's tap scrolls the text there and puts the bar on
  it; the active row is tinted.
- A row's state mark replaces that mention only, then switches it between
  replaced and kept; the bar stays where it is.
- The text scrolls only as far as it takes to show the name between the
  bars, because a fixed target position read as random scrolling.
- A decision that changes the text keeps a visible name under review at the
  same place on screen.
- Candidate ids are source ranges that move, so the bar keeps its place by
  position (`ReviewOrder`).
- The long press on a name, the card's buttons and the keys take their
  actions from `actions(for:)`, so they agree.
- Same as merges into the target, which keeps its alias; the restore
  wording is the core's fullest form.
- Merge hints never merge on their own; a dismissed hint is remembered in
  the case.
- Undo moves the bar back to what it undid and shows no toast.
- Redo is Undo's long press and ⇧⌘Z.
- A Redo shows the Undo toast.
- "All N reviewed · Undo" shows only when this visit of the bar decided or
  changed something, because its Undo would reach an earlier decision.
- Decisions and identity changes go through the same core calls as desktop.
- Snippets come from the current Markdown at core offsets, with only the
  name swapped for its original wording.
- mdoc draws one hint at a time itself, covering no control and gone once
  its action is done, never as a popover, because popovers hid the bar's
  controls.
- The card nudges once to show it swipes.
- Hide name: a selection outside any name is tinted, never grey; the core
  snaps it to whole words and trims punctuation (`hide_target`).
- Hide name opens the review card on the new mark.
- Adjust Words changes a name's words everywhere they fit, keeping the
  identity and alias, as one core step.
- While a name's words are adjusted the text shows the draft as plain text,
  with no candidate link or name decoration.
- Adjust Words' handles stay within the reach the core gives: one paragraph
  or cell, up to Markdown formatting or another alias.
- A tap away from the adjusted words cancels Adjust Words, in a table cell
  too.
- Not a name ends the card's category menu, the long press, Identities'
  menus and Select's ⋯, unstyled, because Undo takes it back.

## Identities

- A tap on a name opens the review bar's full view on its first undecided
  mention.
- A row's state mark replaces a proposed identity, then switches it between
  replaced and kept.
- Replace N is a clear glass capsule at the panel's bottom, not primary,
  because it is a bulk shortcut; in Select, Select's bar takes its place.
- A row swipes left only, showing Replace, Same as… and Keep.
- No full swipe on a row, because Keep sends the original to the AI and
  one swipe back ran Replace.
- A swipe right over the open panel closes the drawer, or puts away a row's
  revealed actions.
- Select ticks rows for Replace, Keep or Same as (into the name mentioned
  most); one Undo takes the selection back.
- In Select, rows have no swipes and no long-press menu; a tap ticks.
- Drag-to-merge works from a ≡ handle in Select.
- A drag-merge follows desktop's rule: the target keeps its alias, another
  category takes the target's, one undo step, no confirmation; the
  selection keeps its other names.
- Mention drags and drop-to-split are desktop only.

## Edit mode

iOS; Android's editor is a later roadmap item.

- One text view serves review, edit mode and the Text/Original switch; the
  pencil, Done and the switch keep the scroll position, caret and selection.
- Entering with a selection brings up the keyboard; with none nothing moves
  and the keyboard waits for a tap.
- Identities are hidden while editing; on iPad the column gives way to the
  Original.
- On iPhone, Original hides the keyboard and keeps edit mode on.
- Edit is disabled while a job runs on the document.
- Smart quotes, smart dashes and auto-pairing are off, because saved bytes
  are the text as typed.
- A command edits only what it changes (line prefixes, wrap markers, a
  task's box), because an edit that runs into a proposed name drops it.
- No table command pads or realigns columns.
- Table controls live in the toolbar with the caret in a cell; nothing is
  drawn in the document.
- A selection in a cell behaves as in ordinary text.
- With the caret in a cell the text scrolls only to keep that cell in view.
- Look Up, Translate, Search Web, Share and Writing Tools stay off in edit
  mode.

## Gestures

- No action exists only as a gesture: each has a visible control. Redo is
  the one exception, as it reverses the visible Undo.
- A sideways swipe anywhere opens a drawer: right the sidebar, left
  Identities.
- A drawer swipe starts after 10 pt of mostly sideways movement; a vertical
  scroll is never taken.
- Content that scrolls sideways (a wide table, a zoomed Original page), the
  review bar, selection handles and long presses keep their own touches.
- A swipe from the screen's 22 pt edges opens a drawer over everything.
- While editing only the left edge opens a drawer, the sidebar.
- While a name's words are adjusted only the edges open a drawer.
- An open drawer closes by a drag toward its side anywhere over it, a drag
  from the sliver or its inner 20 pt, or a tap on the sliver.

## Wording

- Short, plain verbs: Replace, Keep, Same as…, New alias, Undo.
- Toasts name what happened: "12 replaced", "PERSON_3 is now PERSON_1".
- Plain model names ("Name finder").
- English strings are word for word on both platforms; every string lives
  in the String Catalog (iOS) or `strings.xml` (Android).
- US spelling.

## Accessibility

- Every screen works at every Dynamic Type size, up to the largest
  accessibility size, with nothing clipped.
- Every icon-only control has a label.
- Screen-reader support beyond labels waits for the roadmap's
  Accessibility pass; existing VoiceOver and TalkBack code stays but is not
  required in new work.

## Sample document

- `Shared/Core/SampleDocument.swift` is a short contract with made-up names,
  in English or Russian by the phone's language.
- It is Try a sample and the fixture for previews and tests; it never holds
  a real person's data, so its screenshots are safe to share.
- It is editable; Try a sample again asks before restoring an edited copy.
