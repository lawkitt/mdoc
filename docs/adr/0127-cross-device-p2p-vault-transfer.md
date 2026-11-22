# Cross-device Vault transfer by sealed bundle

Status: Accepted, 2026-11-23.

## Why

People scan on a phone and continue on a desktop, and transfer is the only way to a new phone, since mobile files never reach a backup. With no accounts, servers or sync, a transfer is explicit: a Document, a Project or everything.

The sender builds one bundle sealed under a one-time pairing key. Either device shows the key as a QR, since a desktop cannot scan a phone; between desktops it is pasted. The bundle travels over a direct local-network connection, or as a file the user moves. Sealing makes the carrier irrelevant, and neither route adds a dependency or contacts a third party.

A Project merges only when no alias, current or earlier, points at two identities; otherwise it arrives as a separate copy, because rewriting cannot reach text already sent to an AI, and a late reply would restore as the wrong person. Each document records the hash of its last transfer, to tell one-sided edits from two-sided.

## Rejected

- **`iroh` now:** its public relays are for hobby use only; its default build publishes the endpoint id and uses private Apple APIs; it adds 5 MB, `tokio` and 85 crates for what a local connection does alone.
- **Go `tailcat`:** a Go toolchain on four platforms.
- **Background sync:** accounts or daemons.

## Revisit when

Transfers across networks are worth running or paying for a relay: `iroh` without default features then carries the same bundle.
