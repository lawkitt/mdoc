# Mobile review bar

Status: Accepted, 2026-11-05. Amends [ADR 0085](0085-mobile-apps.md)'s review screen and
[ADR 0087](0087-ios-ui-polish.md)'s candidate sheet. Written for iOS first;
Android follows the same decisions.

## Problem

On a phone, review happens in a modal sheet. On build 1 it covers more than
half the screen, stops the text from scrolling, must close before another
name can open, and closes and reopens on Next proposal. Nothing can be
undone, and most of the desktop's review tools (category, rename, Same as…,
the merge hint) are missing. The desktop solved the same problems with one
stable mention bar ([ADR 0055](0055-stable-compact-mention-popup.md)) and a
document history ([ADR 0079](0079-decision-history.md)).

## Decision

### The bar

- **A docked review bar replaces the candidate sheet.** While reviewing, it
  takes the place of the bottom action bar. Nothing dims; the text stays
  scrollable and tappable, and tapping another candidate retargets the bar
  in place.
- **Layout, top to bottom:** a grabber; the progress line ("3 of 15" beside
  a thin track that fills with the decided share); the review card; the
  buttons Undo, Keep and Replace (40 pt high, 14 pt below the card; Replace
  is the one filled button).
- **The review card** has two lines: the category icon and original → alias,
  ending in a mentions chip ("1 of 3 ›") when the identity has more than
  one; then the chips (category ▾, alias ▾, scope All N ▾). A merge hint
  adds a third line while it shows. Slivers of the previous and next cards
  peek at the edges. About 165 pt with the home indicator, about 19% of an
  iPhone 17 Pro. Text stops growing at AX2, with the large content viewer.
- **Tappable means chip.** Anything tappable is a filled capsule; ▾ opens a
  menu; plain text never acts. The name scrolls the text back to it; the
  progress line opens Identities.
- **Expanded.** Swipe up on the grabber for details (the identity's
  mentions with snippets, category, alias, Same as…), capped at about 45% of
  the screen so the active name stays visible. Swipe down returns; swipe
  down on the compact bar, or ×, ends review.

### Deciding and stepping

- **Decisions change the text at once.** Replace and Keep apply on tap, in
  the scope shown; there is no staged Apply on mobile.
- **Scope** is All N by default, This one on demand. It holds within an
  identity and resets on the next.
- **Stepping.** Swipe the card, or tap a sliver, to step in document order
  through every candidate, decided or not; a decided card shows its state
  and can be changed. A decision moves to the next undecided candidate. The
  text glides to keep the name in its top third and the card cross-fades;
  nothing closes and reopens.
- **Focus.** "Review this identity" or "Review 3 people" (from Identities)
  limits stepping to that group; the progress line shows it with ×. When the
  group is done, or on ×, review returns to the whole document.
- **Identities as a map.** While it is open the bar hides; tapping a row
  puts the bar on that identity's first undecided mention.
- **Ending.** The last decision says "All 15 reviewed · Undo", then gives
  way to Share / Copy / Paste reply. Leaving midway keeps every decision.
  Replace N while reviewing replaces the rest and ends review.

### Identity tools

- **Category chip:** each category with the alias it would get.
- **Alias chip:** Rename…, Same as…, New alias.
- **Same as…** is also on a name's long-press menu, an Identities row's
  swipe and context menu, and the merge hint. The picker lists the merge
  hints first ("Looks like"), then the same category, then "Other
  categories ›"; past 8 entries it becomes a searchable sheet. Scope All
  merges the whole identity: the target keeps its alias and restore wording,
  replaced mentions take the target's alias, proposed ones stay proposed,
  and the target's category wins. Scope This one moves only this mention.
  Same as… replaces nothing by itself.
- **Merge hint:** "Looks like Maya Bennett", with Same as and ×. × is
  remembered in the case. Hints never merge on their own (ADR 0080).

### Undo

- Each Replace, Keep, scope change, manual mark, category change, rename,
  merge, Replace all and Keep all is one step, with redo. Reached through
  the bar's Undo, a toast after bulk actions and merges, and the system's
  undo gestures; redo is a long press on Undo. Undo moves the bar back to
  what it undid and the toast names the step. 256 steps, held in memory for
  the open document, in the shared core; joining a case is one step.

### Gestures and feel

- **Swipes:** sideways on the card steps; up and down on the grabber expands
  and collapses. No swipes on the text: a long press on a name opens a menu
  (Replace, Keep, Just this one, Same as…, Change category). In Identities,
  swipe right is Replace, left is Keep plus Same as…; a full swipe acts.
- **Learning them:** visible cues (grabber, peeking slivers), a one-time
  nudge with "Swipe for the next one", and one-time tips (pull up for
  details, long-press a name, shake to undo); Reset tips in Settings.
- **Feel:** a selection tick per card, a soft bump with rubber-band
  resistance at either end, a light haptic on decisions and merges, success
  on All reviewed, warning on undo. An outline on the active mention and on
  every mention in scope. Reduce Motion cross-fades and jumps, with no nudge.

- **Rename…** turns the alias chip into a field. The core's alias rules
  hold; an alias another identity already uses offers Same as instead of an
  error.
- **Scope covers every chip,** as on desktop: All changes the whole
  identity; This one splits the mention off as a new identity. Menu rows
  show the outcome ("Organization → ORG_3"). Belongs to… is the
  exception: an owner belongs to the whole identity, as on desktop.
- **Marking a missed name:** Hide name in the text's edit menu makes a
  proposal with the core's guessed category, and the bar opens on it.

### Long press and expanded view

- **Long-press menu by state,** under a preview header: proposed: Replace,
  Keep, Just this one, Same as…, Change category; replaced: Show original,
  Keep instead, Same as…, Change category; kept: Replace instead, Change
  category.
- **Expanded view:** the compact card, then the identity's mentions with
  context and state, Looks like…, Category, Alias, Belongs to…, and Keep
  all n / Replace all n for the identity.

### iPad

- Wider than about 700 pt: Identities as a standing sidebar, the bar a card
  about 560 pt wide, desktop's keys on a hardware keyboard (Return, K, ←/→,
  ⌘Z, ⇧⌘Z, and ⌘. to end: iOS keeps Esc for itself in places), and drag a
  row onto another to merge. Narrower: the phone
  layout.

### Scope of the run

- **In:** everything above, plus Find in document and mark every match;
  search, Show filter and mention rows in Identities; New alias; Belongs
  to….
- **Later:** What you did, Compare, assigning an unrecognized placeholder,
  adding a name to the case.
- **Never:** find and replace, or any other editing (ADR 0085).

## Invariants

- Review first, read-only source text, core source offsets, and Copy for
  AI bytes unchanged (ADR 0085, 0096).
- Every action has a visible control and a VoiceOver action (Previous and
  Next beside the Candidates rotor), and works at every Dynamic Type size.
  No action exists only as a gesture.
- Opening, stepping and scrolling never close the bar; only ending review
  or deciding the last proposal does.
- Decisions and identity changes go through the same core calls as desktop,
  so desktop and mobile reach the same state for the same steps.

## Consequences

- `crates/mdoc-ffi` gains an undo history (it says today "Mobile has no
  text editing and no undo") and exposes rename, recategorize, the
  single-mention move (`commit_assignment`) and the merge hints it already
  computes (`link_hints`).
- The candidate sheet and its detents go; the Identities sheet stays and
  gains Review this identity and swipe actions.

References: [ios-review-grill-2026-11-05](../design/ios-review-grill-2026-11-05.md)
(rounds 1 to 5).
