# Identities drawer gestures, drag-to-merge and accent grill, 2026-11-08

The maintainer, checking build 811 on the iPhone 17 Pro, found the
Identities drawer's swipes hard to use and asked for an elegant fix, no
Redo toast, a drag-to-merge like desktop's, and later a more elegant
accent than the teal green.

## What the app does today

- Rows use `.swipeActions`: right reveals Replace; left reveals Keep and
  Same as…, and a full left swipe triggers Keep (the first trailing
  action). A decision shows the Undo toast ("Kept X · Undo").
- `SideDrawer` adds a `simultaneousGesture` drag over the whole open
  panel, so a rightward drag on a row both reveals Replace (or closes the
  revealed actions) and slides the drawer shut, and the drawer often wins.
- Rows are already `.draggable` and `.dropDestination`: dropping one name
  on another calls `link` (ADR 0098, "iPad first"), with no highlight or
  hint while dragging.
- Desktop (ADR 0025): group onto group merges, mention onto group moves
  one mention, onto empty space splits; accent outline and a "Link to
  ALIAS" hint; one undo step, no confirmation; the target keeps its alias;
  cross-category adopts the target's category.
- Undo shows a toast "Undid X · Redo" with a countdown
  (`DocumentViewModel+Review.swift`); Redo is also on the Undo button's
  long press, ⇧⌘Z and VoiceOver.
- The accent is desktop's teal (`#336b5c` light, `#82b9a7` dark, ADR
  0099, `src/style.rs`); in dark mode white text on `#82b9a7` reads poorly.

## Round 1

### Q1. Who owns a horizontal swipe in the open drawer

**Decision:** the rows. The drawer closes from ›, a tap on the dimmed
sliver, a swipe that starts on the sliver, or a swipe from its inner edge
(about 20 pt). Swipes over the list always go to the rows.

### Q2. Full swipes

**Decision:** no full swipe for Keep: a left swipe stops at Same as… and
Keep, and the user taps one, since Keep sends the original to the AI.
Replace keeps its full swipe, with a haptic and the Undo toast.

### Q3. Feedback after a swipe

**Decision:** the row's state button animates to its new mark briefly
before the row moves to its new section; the Undo toast stays.

### Q4. No Redo toast

**Decision:** Undo shows no toast; the undone change and a haptic are the
feedback. Redo stays on the Undo button's long press, ⇧⌘Z and VoiceOver. A
Redo still shows its Undo toast.

### Q5. What drag-to-merge does

**Decision:** desktop's rules for name onto name: the dragged name joins
the target, which keeps its alias and restore wording; one undo step, no
confirmation; another category takes the target's. Mention drags and
drop-to-split stay desktop-only.

### Q6. How it looks

**Decision:** while dragging, the row under the finger gets an accent
outline and a one-line hint "Same as PERSON_1"; the dragged row itself
takes no drop; the list scrolls near its edges. On drop, a haptic and the
Undo toast "PERSON_3 is now PERSON_1 · Undo".

### Q7. Discovering it

**Decision:** one quiet hint line in the drawer (ADR 0101's style), "Drag
a name onto another to make them one", only when Looks like has a
suggestion or two names share a category; gone for good after the first
drag-merge or Same as.

## Round 2: the accent

### Q8. Which colour

**Decision:** ink. Near-black `#1c1c1e` fills with white text in light
mode, near-white `#f2f2f7` fills with black text in dark mode, like the
Claude app and Notes, and like the app icon, which is already black and
grey. It never clashes with a category colour and fixes the dark mode's
white-on-pale-green contrast.

### Q9. Desktop

**Decision:** iOS only now; ADR 0099 notes the split. Desktop's accent is
a ROADMAP To-grill item (focus rings, toggles and selection need their
own look); Android follows iOS when it next moves.

## Round 3: ink details and shipping

### Q10. Text-only buttons

**Decision:** primary text colour at medium weight ("Select", "Not now",
"Try a sample"); the toast's action at medium weight too.

### Q11. The name under review

**Decision:** keeps its category tint with an ink outline (black in
light, white in dark).

### Q12. Shipping

**Decision:** two PRs on the Mac, gestures first: (1) the drawer's
gestures, no full-swipe Keep, the state animation, no Redo toast, the
drag-to-merge outline and hint, the one-time hint line (ADR 0106); (2)
the ink accent (ADR 0099 amended, survey refreshed). Reviews by Codex;
then both on the iPhone 17 Pro for the maintainer.

## Round 4: drag-to-merge on iOS

Building Q5–Q7 (PR #717) found that SwiftUI's drag and drop never
delivers a drop inside the List: the List keeps a drag that starts in it
to itself, so a row lifts and the list scrolls, but no row takes the drop.

### Q13. How a drag starts

**Decision:** (a) in Select each row shows a ≡ handle at its trailing
edge; dragging the handle onto another name makes them one, with the
app's own gesture instead of the system's drag and drop. The hint line
says "In Select, drag ≡ onto a name to make them one"; VoiceOver has Make
one with… on the handle.

### Q14. What the target keeps

**Decision:** its alias. The restore wording follows the core's rule, the
fullest form of the names (#388), not the target's; ADR 0106 says so.

