# App store licence permission

Status: Accepted, 2026-10-31.

## Problem

mdoc is GPL-3.0-or-later. Apple's App Store terms add usage restrictions
that the FSF holds are incompatible with the GPL; VLC was removed from the
App Store over this in 2011. The mobile apps ([ADR 0085](0085-mobile-apps.md))
ship through the App Store and Google Play.

## Decision

- mdoc stays GPL-3.0-or-later and gains an additional permission under
  GPLv3 section 7: the program may be distributed through app stores whose
  terms add usage restrictions, provided the corresponding source stays
  available under the GPL.
- The permission text goes in `LICENSE` beside the GPL and in the mobile
  apps' about screens.
- Outside contributions need a sign-off (DCO) that agrees to the
  permission. Until the permission lands, all non-bot commits are the
  maintainer's.

## Invariants

- `cargo deny` keeps third-party GPL code out of the crates the mobile apps
  link, so the permission never has to cover someone else's code.

## Consequences

- The licence of earlier releases is unchanged.
- A dependency under the GPL or LGPL in a mobile-linked crate is refused
  unless its holder grants the same permission.

References: [interview](../design/mobile-grill-2026-10-31.md).
