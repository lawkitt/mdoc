# Cross-device P2P vault transfer via iroh

Status: Accepted, 2026-11-22.

## Why

Users scan or review documents on iPhone/Android and continue working on macOS/Windows (and vice versa), while mdoc promises no accounts, no cloud storage, and no background sync. On-demand, user-initiated transfer of a Project or Document solves this cleanly without distributed sync conflicts.

We use pure-Rust `iroh` (`iroh` + `iroh-blobs`) directly in the shared core rather than Tailscale's Go-based `tailcat`, achieving the same zero-account, direct P2P + relay-fallback UX without adding a Go/`gomobile`/cgo toolchain across four platforms and CI. Each send starts a one-time session encoded as a QR code and short copyable code carrying the peer address, ephemeral auth secret, and manifest hash. The bundle streams chunked over `iroh` QUIC double-encrypted with an ephemeral `AES-256-GCM` key, includes Source Originals by default (with an opt-out toggle for slow cellular), previews size and contents on the receiver, and expires after one transfer, cancel, or 5 minutes. Re-sending an existing `ProjectId` merges non-destructively (union of stable aliases, conflict copy if a `DocumentId` diverged). `iroh` prefers direct LAN/P2P and falls back to relays unless the user enables "Local network only".

## Rejected

- **Embedding Go `tailcat`:** adds Go cross-compilation (`gomobile`/cgo) to desktop, iOS, and Android builds and CI.
- **Continuous background sync:** requires accounts or always-on daemons and complex conflict resolution.
- **Strictly LAN-only:** breaks cellular-to-office transfers; offered instead as a Settings toggle.

## Revisit when

Traffic volume or enterprise privacy requirements justify operating a self-hosted `iroh-relay` as the default.
