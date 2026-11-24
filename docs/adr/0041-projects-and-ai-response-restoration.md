# Projects and AI-response restoration

Status: Accepted, 2026-10-11.

## Why

Users send pseudonymized Markdown to ChatGPT or Claude and get back text
full of aliases. A mapping that dies with its document can't restore the
reply, and the same client gets different aliases in every document. A
Project is a named mapping shared by its documents, kept in the Vault
([ADR 0125](0125-unified-encrypted-vault-and-projects.md)) and encrypted
with AES-256-GCM under a key the OS holds (the Keychain, DPAPI or Secret
Service); without a keyring, the Vault is unavailable. This protects
against other people and casual inspection, not against malware running as
the same user. Aliases are never reused in a Project, so an old reply still
restores. Pseudonymized text remains personal data, and the UI never calls
a Project or its output anonymous.

## Rejected

- **A plaintext key file or fallback**: it would undo encryption at rest.
- **Restoring partial matches**: a wrong name in a legal reply is worse than
  an unresolved token the user assigns.
- **Mappings kept only per document**: aliases would differ across a
  client's documents.

## Revisit when

Projects need to reach a second device other than by an explicit transfer
([ADR 0127](0127-cross-device-p2p-vault-transfer.md)), which changes the
key.
