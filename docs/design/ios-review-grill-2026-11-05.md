# iOS review redesign grill, 2026-11-05

The maintainer tested build 1 (`ios-v0.1.0-rc.2`) on an iPhone 17 Pro and
found the review flow neither compact nor smooth:

1. The candidate sheet covers more than half the screen.
2. While it is open the document can't scroll.
3. Switching to another name means closing the sheet first.
4. Next proposal closes the sheet and opens a new one.
5. There is no undo.
6. Much of the desktop's review power is missing.

Points 1 to 4 share one cause: review happens in a modal sheet, while the
desktop uses one stable mention bar that follows the active mention
([ADR 0048](../adr/0048-workspace-shell-redesign.md),
[ADR 0055](../adr/0055-stable-compact-mention-popup.md)). Point 5 has a core
cause too: `crates/mdoc-ffi` has replace, keep, Replace all, mark and link,
and no undo, rename or category change.

Decisions land in [ADR 0098](../adr/0098-mobile-review-bar.md).

## Desktop features against iOS

From the code and ADRs on `main` at 1e709641. ✅ has it, ◐ partial, ❌
missing on iOS.

| Desktop | iOS |
|---|---|
| One popup follows the active mention (ADR 0055) | ◐ a modal sheet per candidate |
| Replace / Keep, This one \| All N | ✅ |
| Enter applies and moves to the next proposal | ◐ Next proposal opens the first one left, from the top |
| Previous proposal; ‹ 1/5 › within an entity | ❌ |
| Undo and redo, 256 steps; What you did (ADR 0079) | ❌ |
| Category menu with each category's alias | ❌ |
| Belongs to… (owner of a contact or identifier) | ❌ |
| Rename an alias; Same as…; New alias (split) | ❌ (`link` is in the FFI, unused) |
| Merge hint (ADR 0080); drag to merge | ❌ |
| Replace on a selection: guessed category, proposed, Cancel (ADR 0024, 0025) | ◐ Person by default, applied at once, no Cancel |
| Find and replace (ADR 0063) | ❌ (ADR 0085: no editing) |
| Panel: search, Show filter, mention rows, summary, Rescan | ◐ Identities sheet; rows open the first mention |
| Scope outline, apply/keep motion (ADR 0025, 0032) | ❌ |
| Compare: original wording beside pseudonymized | ❌ ("Original" is the page image) |
| Assign an unrecognized placeholder; Next unrecognized | ❌ (count only) |
| Add a name to the case | ❌ |

ADR 0085 excludes editing. It lists rename, category, merge and owner
nowhere and gives no reason, so adding them overturns no decision.

## Round 1

All seven recommendations accepted.

1. **Review surface: a docked review bar.** A compact bar, about
   110–130 pt on a 17 Pro, takes the place of the bottom action bar while
   reviewing. Nothing dims; the text stays scrollable and tappable, and
   tapping another candidate retargets the bar in place.
2. **Text changes at once, with undo.** One tap per name, as today; no
   staged Apply.
3. **Undo.** Every Replace, Keep, scope change, manual mark, merge, and
   Replace all / Keep all (one step each). Through an Undo button in the
   bar, a toast after bulk actions, and `UndoManager` (shake, three-finger
   swipe), with Redo. In memory, for the open document. The history lives
   in the shared core so Android gets the same. Remove reply and Delete
   document are not on it: they already ask first. What you did comes later.
4. **Stepping.** Document order. After Replace or Keep the bar moves on
   to the next proposal: the text scrolls and the bar's content cross-fades,
   with no close and reopen. ‹ › and "3 of 15". With none left the bar says
   "All reviewed" and gives way to Share / Copy.
5. **Scope.** All N by default, with a compact This one. The choice holds
   while stepping within an identity and resets to All on the next one.
6. **Feel.** A light haptic on Replace and Keep, success on All reviewed,
   warning on undo. An outline on the active mention, and on every mention
   the scope covers. Cross-fade and slide; none with Reduce Motion.
7. **Where it lands.** iOS 0.2's headline, with a platform-neutral ADR for
   Android and core work shared with the desktop. The bar and undo merge
   first, as their own PRs, so they reach TestFlight early.

The maintainer added swiping to the grill: how it works, and how the user
learns it is there.

## Round 2

All accepted, except that the maintainer reopened where ‹ 3 of 15 › sits:
right above Replace it invites a mis-tap (round 3).

8. **Compact bar.** Two rows, about 120 pt with the home indicator. Row 1
   says what is under review: category icon, original → alias, position.
   Row 2 acts: Undo, scope chip, Keep, Replace (the one filled button). The
   Found box and the explanation line go; the explanation becomes a
   one-time tip. Text stops growing at AX2, with the large content viewer.
9. **Sideways swipe on the bar steps** between proposals, following the
   finger and springing back at the ends; the text scrolls along. Deciding
   stays a tap.
10. **Up and down.** A grabber. Swipe up expands the bar into details
    (category, alias, Same as…, this identity's mentions), capped at about
    45% of the screen so the active name stays visible; swipe down returns.
    Swipe down on the compact bar, or ×, ends review.
11. **Elsewhere.** No swipes on the text: a long press on a candidate opens
    a menu (Replace, Keep, Just this one, Same as…, Change category). In
    Identities, swipe right is Replace (blue), left is Keep (gray) plus
    Same as…; a full swipe acts, with a haptic at the threshold. Every swipe
    has a visible button and a VoiceOver action.
12. **Learning the gestures.** Visible cues (grabber, paging cues); a
    one-time nudge of the bar's content with "Swipe for the next one"; TipKit
    tips, once each (pull up for details, long-press a name, shake to undo);
    Reset tips in Settings › About. Reduce Motion: a caption, no nudge.
13. **Desktop features by tier.**
    - Tier 1, with the bar: previous and per-identity stepping, change
      category, rename alias, Same as… (merge, via `link`), the merge hint,
      and a marked selection becoming a proposal with a guessed category.
    - Tier 2, iOS 0.2: Find in document and mark every match; search, Show
      filter and mention rows in Identities; New alias (split); Belongs to….
    - Later: What you did, Compare, assigning an unrecognized placeholder,
      adding a name to the case.
    - Never: find and replace or any editing (ADR 0085).
14. **Ending.** The last decision gives a success haptic and "All 15
    reviewed · Undo" for about 2 s, then Share / Copy / Paste reply.
    Leaving midway keeps every decision; the bar returns to Review /
    Replace N with "12 left". Replace N while reviewing replaces the rest,
    says "12 replaced · Undo" and ends review.

## Round 3

All accepted.

15. **Position and stepping.** No ‹ › buttons. "3 of 15" is a small caption
    beside a thin progress track under the grabber, far from the decision
    buttons. The card under review shows slivers of its neighbors at both
    edges: swipe the card, or tap a sliver, to step. VoiceOver gets
    Previous and Next actions beside the Candidates rotor.
16. **Swiping walks every candidate,** decided or not; a decided card shows
    its state and can be changed. A decision jumps to the next undecided
    one. The counter counts all candidates; the track fills with the
    decided share. A skipped one stays proposed and counts as left.
17. **What is tappable.** Anything tappable is a filled capsule chip; a chip
    with ▾ opens a menu; plain text never acts.
    - Category chip: each category with the alias it would get; applies in
      the shown scope, one undo step.
    - Alias chip: Rename… (in place, bar above the keyboard), Same as…, New
      alias.
    - Scope chip: All N or This one.
    - The name scrolls the text back to it. The progress line opens
      Identities.
    - Undo, Keep and Replace are buttons, 40 pt high, 14 pt below the card.
18. **Height.** A two-line card (name, then chips): about 165 pt with the
    home indicator, about 19% of a 17 Pro. Expanded stays capped at 45%.
19. **Swipe feel.** A selection tick per card, a soft bump with rubber-band
    resistance at either end, the text gliding to keep the name in its top
    third. Reduce Motion: cross-fade and jump.

## Round 4

All accepted. The maintainer asked how to move between grouped mentions
and how Same as… works.

20. **One identity's mentions.** When the identity under review has more
    than one mention, the card's first line ends in a mentions chip,
    "1 of 3 ›". Each tap scrolls the text to the identity's next mention,
    cycling round; the card stays on the identity. Expanded, the same
    mentions show as a list with snippets. Every mention in scope is
    already outlined (decision 6).
21. **Focus on one group.** Start it from an identity's row in Identities
    ("Review this identity") or a category header ("Review 3 people").
    The progress line then reads "ORG_1 · 1 of 3 ×" or "People · 2 of 5 ×",
    and swipes and decisions stay inside the group. When the group is
    done, or on ×, review returns to the whole document at the same place.
    The default walk stays top to bottom.
22. **Identities as a map.** Tapping the progress line opens Identities,
    and the bar hides while it is open. Tapping a row closes it and puts
    the bar on that identity's first undecided mention, or its first
    mention if all are decided. The list keeps its large height at AX sizes
    and on iPad (#598).
23. **Where Same as… lives.** The alias chip's menu, the long-press menu on
    a name, a row's swipe-left action and context menu in Identities, and
    the merge hint. Later, on iPad: drag one row onto another, as on desktop.
24. **The picker.** A submenu: the core's merge hints for this identity
    first, marked "Looks like"; then the same category in order of first
    mention, each row "Maya Bennett · PERSON_1 · 4"; last, "Other
    categories ›". With more than 8 candidates the item opens a searchable
    list in a sheet.
25. **What merges.** Scope All merges the whole identity (`link`, core
    `merge_identity`): the target keeps its alias and the wording a reply
    restores to; replaced mentions take the target's alias, proposed ones
    stay proposed; across categories the target's category wins. Scope
    This one moves only this mention to the target, as desktop's scoped
    Same as… does. Same as… replaces nothing by itself: the card stays,
    showing the target's alias, and Replace is the next tap.
    - Fact found after the round: the core already has the single-mention
      move (`Review::commit_assignment`, used by desktop's
      `MappingAction::Scoped`), so This one needs the FFI to expose it, not
      new core logic.
26. **Feedback.** The alias chip cross-fades to the target's alias and the
    scope chip updates ("All 6"); the outline spreads to every linked
    mention; a light haptic; a toast "Same as Maya Bennett · Undo", one
    undo step. The way back out is New alias (tier 2).
27. **The merge hint.** A third card line, "Looks like Maya Bennett", with
    a Same as chip and ×. The card grows about 28 pt only while it shows.
    × dismisses it, stored in the case when there is one, as on desktop.
    Hints never merge by themselves (ADR 0080).

## Round 5

All accepted. The maintainer merged tiers 1 and 2 into one run and added
two topics: how to keep UI work iterable with agents, and a theme switcher
with a nicer Settings screen and home screen (round 6).

28. **Rename….** The alias chip becomes a field, keyboard up, a small bar
    above it; Return applies, Cancel restores. The core's rules hold:
    letters, digits, `_` and `-`, up to 128 characters; aliases the case
    retired are refused. An alias another identity uses shows "PERSON_2 is
    Maya Bennett" with a Same as chip instead of an error (desktop:
    "Choose the existing entity below to use its alias").
29. **Scope for category and rename,** as desktop: All changes the whole
    identity; This one splits the mention off as a new identity. Menu rows
    show the outcome ("Organization → ORG_3"); the toast says "Split off as
    ORG_3 · Undo".
30. **Undo.** Undo moves the bar back to what it undid and scrolls there;
    the toast names the step ("Undid Replace · Riverside Holdings"). Redo is
    a long press on Undo, in the shake prompt and the three-finger swipe.
    256 steps, as desktop; cleared when the document closes. Joining a case
    is one step (as desktop's `case_sync`). Remove reply and Delete document
    stay outside it.
31. **Marking a missed name.** Select text, Hide name in the edit menu: it
    becomes a proposal with the core's guessed category (Person otherwise),
    the bar opens on it, Replace applies. Undo removes the mark. Build 1
    applies at once; that changes.
32. **Long-press menu by state,** with a preview header (name and alias):
    proposed: Replace, Keep, Just this one, Same as…, Change category;
    replaced: Show original (a callout), Keep instead, Same as…, Change
    category; kept: Replace instead, Change category.
33. **Expanded view,** at most 45%: the compact card stays on top; then
    Mentions (n) rows with about 40 characters of context and a state icon,
    tap to jump; Looks like…; Category and Alias rows with menus; Belongs
    to…; a footer with Keep all n and Replace all n for the identity.
34. **iPad.** Wider than about 700 pt: Identities as a standing sidebar,
    the bar a card about 560 pt wide at the bottom centre, and desktop's
    keys on a hardware keyboard (Return Replace, K Keep, ←/→ step, ⌘Z and
    ⇧⌘Z, Esc end). Narrower: the phone layout. Drag one row onto another to
    merge.
35. **Android.** The new core calls are tested once in the core's parity
    tests; Android builds its own bar from ADR 0098, with Material
    conventions, in its own milestone.
36. **Delivery.** One run, tiers 1 and 2 together, each PR reaching
    TestFlight on its own: core (undo, rename, category, single-mention
    move, hints, owner) with parity tests; the bar in place of the sheet
    (stepping, Undo, haptics, outlines); the chips and long-press menus;
    peeks, progress line, mentions chip, tips, Reduce Motion; focus,
    expanded view, merge hint, marking a missed name; tier 2 (Find in
    document, Identities search, Show filter and mention rows, New alias,
    Belongs to…); iPad.
37. **Done means:** each PR refreshes the Simulator UI survey (light, dark,
    AX sizes) and records stepping and undo; UI tests drive every gesture
    through its VoiceOver action; the maintainer checks each TestFlight
    build on the iPhone 17 Pro. Agents never touch the phones.

## Round 6

All accepted.

Facts first: the tree held 38 MB of dated iOS screenshot sets (8) and
18 MB of Android ones; `just ios-survey` defaulted to "iPhone 17"; iOS had
no design tokens, no fixture-fed previews and no theme setting (desktop has
System, Light, Dark); Settings showed raw model names and the phone's RAM;
Home's case line nearly repeated the title.

**Keeping UI work iterable with agents**

38. **One living guide,** `docs/mobile-ui.md`, for both mobile apps: tokens,
    components, gestures, haptics, motion, wording and accessibility, each
    with a code pointer, and an iOS and an Android column where they differ.
    ADRs keep the why; grill notes go once absorbed. Every UI PR updates it;
    agents read it before UI work.
39. **Tokens and components in code:** `apps/ios/mdoc/Design/` with
    `Tokens.swift` (spacing 4/8/12/16/24, radii, type styles, named colors
    in the asset catalog) and one file per component (`Chip`, `Card`,
    `Toast`, `StatusPill`, `IconTile`). Screens compose only these. Android
    mirrors them in `ui/theme` under the same names.
40. **Previews instead of sketches:** a `#Preview` for every component and
    screen state, light, dark, AX5 and iPad, fed by `PreviewFixtures.swift`
    (a made-up legal document).
41. **Snapshot tests** of components and key states (review bar, Home,
    Settings) with swift-snapshot-testing, in the existing iOS job.
42. **Screenshots in the tree:** one `current/` survey per platform,
    replaced by each UI PR; before and after go in the PR description; the
    dated folders leave the tree, linked by permalink; `just ios-survey`
    defaults to the iPhone 17 Pro.
43. **A sample document for everyone:** a short contract with made-up
    names, "Try a sample" in the empty state and About; the same text as the
    test fixture. Device screenshots of it are shareable and reproducible.
44. **The loop:** a note or a screenshot of the sample becomes a `ui` issue
    (what's wrong, what good looks like); an agent changes components and
    updates previews, snapshots and `current/`; the PR carries before and
    after; the maintainer checks on TestFlight. Grill only for a pattern
    the guide lacks, and the guide then gains it.

**Theme and Settings**

45. **Theme:** Settings › Appearance › Theme, Automatic / Light / Dark,
    applied at once to every sheet, stored on the device. Android too.
46. **Settings:** inset grouped with colored icon tiles. Appearance (Theme,
    Show Markdown); Review (Haptics, Reset tips); Models with plain names
    ("Name finder", "Text recognition (English, Russian)") and "Installed ·
    632 MB", a tap opening details (technical name, Allow cellular, Remove
    or Download again); Privacy ("Documents stay on this iPhone"); About
    mdoc (version, licenses, Try a sample). Memory moves into the name
    finder's details as "This iPhone: 12 GB · enough to find names".

**Home**

47. **Row:** a source icon tile (PDF, photo, scan, pasted text); the title on
    one line; state and time on line 2, colored with an icon ("11 to review ·
    17:24" orange, "Ready to share · Yesterday" green, "Reply restored"
    blue).
48. **Cases:** a flat recent list with the case as a small colored tag on
    line 2, and a case filter in the toolbar from two cases up.
49. **Order, search, menus:** most recently touched first; search over
    titles and cases from 8 documents; a long press gives a preview and
    Rename, Move to case, Share, Delete.
50. **Adding:** a bottom toolbar with a prominent Scan and a + menu for
    Paste, Files and Photos; Settings stays top left.
51. **Empty state:** three large tiles (Scan, Files, Paste) and "Try a
    sample" under them; the setup card stays above while models download.

## Round 7

All accepted; the frontier is empty and the maintainer asked to build it
all. Facts first: iOS set no accent (system blue); desktop's is teal green
(`#336b5c` light, `#82b9a7` dark); document text was the system font at
Dynamic Type sizes.

52. **Accent:** desktop's teal green.
53. **Document font:** System (default) or Serif (New York), in Appearance.
54. **Sample:** English and Russian, chosen by the phone's language.
55. **Order:** the foundation (guide, tokens, components, fixtures,
    snapshots, screenshot cleanup, survey device, sample) first; then two
    lanes, review (core, bar, chips and menus, peeks and tips, focus and
    expanded view and hint and marking, tier 2) and look (theme, accent,
    font, Settings, Home); iPad last.
56. **Records:** ADR 0098 (review bar) and ADR 0099 (mobile UI system);
    an iOS 0.2 roadmap section; Android takes the same decisions in its own
    milestone.
