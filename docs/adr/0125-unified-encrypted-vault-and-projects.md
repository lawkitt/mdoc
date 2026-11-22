# Unified encrypted Vault and Projects

Status: Accepted, 2026-11-22.

## Why

Desktop's loose-file model left source documents and Markdown scattered across folders with brittle paths in `.case` files, while mobile left Markdown and originals unencrypted in its sandbox and auto-created throwaway cases on every import. Renaming **Case** to **Project** ends the collision with `Case number` (`CASE_1`) and makes Projects explicit multi-document containers.

A shared GUI-free crate (`crates/mdoc-vault`) becomes the single store for desktop, iOS, Android, the menu bar shelf, MCP, and device transfer. Everything at rest is encrypted with `AES-256-GCM` under the existing 32-byte OS Keychain/DPAPI key using opaque UUID filenames: single-shot `AES-256-GCM` for structured files (`manifest.enc`, `projects/`, `docs/*/doc.enc`, `replies/`) and 1 MiB chunked `AES-256-GCM` for `original.enc` so large scans never OOM phones. Each Project holds **Documents** (one editable Markdown, per-mention review state, cached `ready_snapshot`, optional immutable Source Original) and **AI Replies**; standalone documents live **Unfiled** with document-local mappings. Vault docs autosave; desktop keeps external `.md` open/export.

## Rejected

- **SQLCipher / SQLite:** adds C cross-compilation across four platforms and risks iOS App Group `0xdead10cc` lock kills in `RestoreExtension`.
- **HKDF + per-object DEK envelope:** unnecessary complexity for a single-user local threat model; a future passphrase only wraps the single 32-byte key.
- **Reference Attachments (3rd item type):** turns mdoc into a generic file cabinet outside AI preparation.
- **Legacy `.case` migration:** early development with no production users; clean break.

## Revisit when

Multi-user shared vaults or full-text search across thousands of encrypted documents require a database index or per-project key sharing.
