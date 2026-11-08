# Mobile Identities drawer

Status: Accepted, 2026-11-06. Amends [ADR 0098](0098-mobile-review-bar.md)
(Identities as a map) and [ADR 0085](0085-mobile-apps.md) (the Identities
sheet). Written for iOS first; Android follows the same decisions.

## Problem

On a phone, Identities is a sheet. At half height it fits two names under
a title row, a search field and a four-way filter. Each row takes three
lines and a pill with words, and rows are grouped by category, so what is
left to decide is spread over the sections. Bulk work (replace these
three, merge these two) takes one swipe per name. When the same person
appears twice, for example once in Latin and once in Cyrillic letters,
nothing on the screen suggests merging them, and doing it by hand is a
swipe and a picker per name.

## Decision

### Where it lives

- On iPhone, a **drawer from the right edge**, the mirror of the sidebar
  ([ADR 0102](0102-mobile-sidebar.md)): the documents on the left, this
  document's names on the right. It opens from the right edge or the 🪪
  button with the same motion (`SideDrawer`, trailing). On a wide iPad it
  stays the standing right column.
- A tap on a name closes the drawer and opens the review bar's full view
  on its first undecided mention: the full view is the identity's detail
  ([ADR 0101](0101-mobile-review-bar-polish.md)).

### Layout

- **Top:** a ring that fills as names are decided, "Identities", "9 names
  · 4 left", a search circle that widens into a field, Select, and › to
  close. Below, a filled **Replace 4** for the names left.
- **Looks like:** a card per merge hint the core offers ("Maya Bennett ·
  Ms Bennett", the two aliases), with Same as and "Not the same" (×).
- **Category chips:** All, then each category with its count, as a filter.
- **Sections by what is left:** *To review*, then *Replaced*, then *Kept*
  (folded until opened or searched).
- **Rows:** the category icon in its colour; the name, wrapped at words;
  then the alias, "3×" and its other spellings in grey. A trailing
  round state button, the full view's state mark: a tap replaces a
  proposed name, then switches it between Replaced and Kept. The toast
  offers Undo.
- Swipes (right Replace; left Keep and Same as…), the long press and
  iPad's drag-to-merge stay as ADR 0098 set them.

### Select

- Select ticks rows. A bar at the bottom then offers Replace, Keep, and
  Same as (two or more), which merges them into the one mentioned most:
  it keeps its alias and restore wording. Each name is one undo step, and
  the toast's Undo takes the whole selection back.

### Not in this run

- Hinting transliterated pairs ("Ivanov" and "Иванов"): merge hints treat
  transliteration as no signal on purpose, and a hint would change desktop
  too (ROADMAP, Later). Select and Same as fix such a pair by hand.

## Invariants

- Every action has a visible control and a VoiceOver action: rows read
  "name, also spellings, category, alias, mentions, state", with custom
  actions for Replace all or Keep all, Review this identity and Same as….
- Decisions go through the same core calls as the review bar and desktop.

## Consequences

- `IdentitiesView` is rebuilt over the document's model;
  `IdentitiesPanel` adds what its rows open; `IdentitiesDrawer` wraps the
  iPhone screen.
- `DocumentViewModel+Identities` holds the list's actions (`toggle`,
  `decide(identities:)`, `merge`, `openIdentity`).

References: [ios-sidebar-grill-2026-11-06](../design/ios-sidebar-grill-2026-11-06.md)
(round 2, Q8 to Q14).
