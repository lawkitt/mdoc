# Cases and AI-response restoration

Status: Accepted, 2026-10-11.

## Why

Users send pseudonymized Markdown to ChatGPT or Claude and get back text
full of aliases. A mapping that dies with its tab can't restore the reply,
and the same client gets different aliases in every document. A case is a
named mapping saved across documents. Creating one is the user's consent to
store originals on disk, encrypted with AES-256-GCM under a key the OS
holds (the Keychain, DPAPI or Secret Service); without a keyring, cases are
off. This protects against other people and casual inspection, not against
malware running as the same user. Aliases are never reused in a case, so an
old reply still restores. Pseudonymized text remains personal data, and the
UI never calls a case or its output anonymous.

## Rejected

- **A plaintext key file or fallback**: it would undo encryption at rest.
- **Restoring partial matches**: a wrong name in a legal reply is worse than
  an unresolved token the user assigns.
- **Mappings kept only per document**: the reply can't come back to a
  closed tab, and aliases would differ across a client's documents.

## Revisit when

Cases need to reach a second device, which changes both consent and the
key.
