# Unified encrypted Vault and Projects

Status: Accepted, 2026-11-22.

## Why

Desktop scattered documents and originals behind brittle paths in `.case` files; mobile made a throwaway case per import and encrypted only mappings. **Project** replaces **Case**, ending the clash with `Case number`.

`crates/mdoc-vault` is the one store for desktop, mobile, the shelf, MCP and transfer: an `AES-256-GCM` file per object under the existing OS-held key. Object files are the only truth; `manifest.enc` is a cache rebuilt from them, so a crash between writes costs nothing. Each Source Original has its own wrapped random key, so chunk nonces are plain counters. Identities carry stable ids, and a saved AI Reply records which identity each alias meant. Readiness is re-checked against the Project's names whenever text leaves.

Costs accepted: mdoc keeps every import, original and mapping until deleted, where a case was the consent to store; the key is the only way in, so desktop offers a recovery key and Export Project.

## Rejected

- **SQLite / SQLCipher:** C cross-compilation; lock kills in the iOS Restore extension.
- **One key with random nonce prefixes for originals:** passes NIST's collision bound near 92,000 files.
- **Editing external `.md` in place:** a second document model; a `.md` imports a copy.
- **A desktop tab bar at first:** tabs restore the open/closed split the tree removes.
- **Migrating `.case` files:** few installs; old folders stay untouched.

## Revisit when

Shared vaults or full-text search over thousands of documents need an index or per-Project keys.
