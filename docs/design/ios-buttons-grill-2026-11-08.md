# Big buttons grill, 2026-11-08

The maintainer, checking build 820 on the iPhone 17 Pro, asked how to make
the app's big buttons more elegant, with proper colours and styles, and
wanted sketches. The same check found four review bar bugs (the text not
scrolling to the card's name, a swipe that isn't a carousel, a bar that
changes height, a right swipe from Identities opening Documents); those
are fixed in their own PR and are not decided here.

## What the app does today

- A filled button is `.accentProminent()`: the system's
  `.borderedProminent` tinted ink (`#1c1c1e` light, `#f2f2f7` dark, ADR
  0099 as amended earlier today), with on-accent words.
- Other big buttons are `.bordered`: a grey capsule.
- The review bar's Undo, Keep, Replace and Next are 40 pt high
  (`Size.barButton`), under the 44 pt hit target the rest of the app keeps.
- Identities' "Replace N" is a full-width filled slab under the header:
  the loudest thing in the drawer. In dark mode it and Next are
  near-white slabs.
- The top bar's ☰, Text/Original and ⋯ are already Liquid Glass capsules.

Three options were sketched (light and dark): A today's solid ink slabs;
B Liquid Glass; C one solid button and the rest plain text, with "Replace
N" as a small pill in the header row.

## Decisions

### Q1. Material

**Decision:** Liquid Glass. The primary button is ink-tinted prominent
glass (`.glassProminent` tinted ink); other big buttons are clear glass
(`.glass`) with ink words.

**Why:** it matches the glass buttons already at the top, softens the
near-white slabs in dark mode without a new palette, and as the system's
style it adapts to dark mode, Increase Contrast and Reduce Transparency
on its own.

### Q2. Identities' "Replace N"

**Decision:** full width, clear glass with ink words: a secondary weight,
no longer filled.

**Why:** it is a bulk shortcut that skips looking at each name, and
looking is what the app is for; it stays easy to find without being the
loudest thing in the drawer. A pill in the header (option C) would crowd
the ring, title, search, Select and ›.

### Q3. Height

**Decision:** 44 pt for every big button, the review bar's included
(`Size.barButton` 40 → 44). The compact review bar grows by 4 pt once and
then keeps one height (the height bug's fix).

### Q4. Undo

**Decision:** a clear glass circle, like the other secondary buttons.

**Why:** a bare icon beside two capsules reads as disabled when it isn't.

### Q5. Scope

**Decision:** every big button in the app: the review bar, Identities
(Replace N and the Select bar), the bottom action bar, setup cards and
Documents. Two shared styles in `apps/ios/mdoc/Design` replace the
`.accentProminent()` and `.bordered` calls; screens use only those.
Recorded as an ADR 0099 amendment; docs/mobile-ui.md changes with the PR
that implements it.

### Q6. Order

**Decision:** restyle after the review bar bug-fix PR merges, as one PR.

**Why:** both edit `ReviewBar.swift`; waiting avoids a conflict in the
carousel code for a change of about an hour.

## Invariants

- At most one prominent (ink glass) button per view.
- No literal colours or materials in screens: the two shared styles only.
- Disabled buttons keep the secondary colour for their words.
- Hit targets never under 44 pt.
