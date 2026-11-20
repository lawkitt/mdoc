# App store licence permission

Status: Accepted, 2026-10-31.

## Why

mdoc is GPL-3.0-or-later, and Apple's App Store terms add usage
restrictions that the FSF holds incompatible with the GPL; VLC left the App
Store over this in 2011. So mdoc adds a GPLv3 section 7 permission: it may
be distributed through app stores whose terms add usage restrictions,
provided the corresponding source stays available under the GPL. The text
is `LICENSE-APP-STORES.md`, beside the unchanged `LICENSE`, so GitHub still
detects GPL-3.0. Outside contributions need a DCO sign-off that agrees to
the permission, and `cargo deny` keeps third-party GPL and LGPL code out of
the crates the mobile apps link, so the permission never has to cover
someone else's code. Earlier releases keep their licence.

## Rejected

- **Staying off the app stores**: phones are where lawyers use AI apps.
- **A GPL or LGPL dependency in a mobile-linked crate** without its holder
  granting the same permission.

## Revisit when

The FSF or the stores change their terms, or a dependency the phones need
is GPL-only.
