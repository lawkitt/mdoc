# A quiet desktop: a library Home, and colour only on names

Status: Accepted, 2026-11-27. Built in 0.5.0 ([#1357](https://github.com/lawkitt/mdoc/issues/1357)); until each screen lands, [the desktop spec](../specs/desktop.md) describes what ships.

## Why

The desktop app grew screen by screen: 11 and 12 px text, frames inside frames, three accent systems, and a Home that says "Hello" whatever the Vault holds. The interface competed with the names a lawyer must check.

## Decision

1. **Colour belongs to names.** The interface is monochrome on neutral greys. The only saturated colour in a window is a found name's category underline and the blue of a replaced one. Find matches, spelling marks and the amber "needs review" badge are the exceptions.
2. **Home is the library.** It shows the Vault as cards: Projects as stacks, documents with a miniature of their text, names marked or replaced. The sidebar stays the only tree.
3. **One component kit.** Sizes, radii, greys and shadows come from tokens in `apps/desktop/src/ui`, ported from Quiet.md's kit (MIT) with credit.

## Rejected

- **Home as a launcher:** it hid the user's own documents.
- **An accent colour for controls:** any hue beside the names dilutes them, and blue already means "applied".
- **Quiet.md's editor and icons:** lawyers correct a word and mark a name; the icons' licence forbids redistribution.
- **A "classic" look as a switch:** it doubles every render and check by eye.

## Revisit when

Thumbnails prove too slow or too revealing on a shared screen, or a state needs a colour that a mark, a word or a count cannot carry.
