# Cases and AI-response restoration

Status: Accepted, 2026-10-11.

## Problem

Users send pseudonymized Markdown to ChatGPT or Claude and get back text full
of `PERSON_1`-style aliases. A mapping that dies with its tab cannot restore
the reply, and the same client gets different aliases in every document.

## Decision

- **Cases.** A case is a named mapping saved across documents; a document
  belongs to at most one. Creating a case is the user's consent to store
  originals on disk; without a case, mappings stay in memory and end with the
  tab.
- **Storage.** One `<id>.case` file per case in `mdoc/cases/`: `MDOCCASE1`, a
  12-byte nonce, then AES-256-GCM over JSON, authenticated with the case id.
  The key is held by the OS: the Keychain on macOS, a DPAPI-protected
  `key.dpapi` on Windows, a Secret Service item on Linux. Without a keyring,
  cases are off.
- **Restore.** Aliases in a pasted AI reply are replaced by each identity's
  primary wording, with lenient separators and Markdown wrappers kept.
  Restore never guesses: partial matches never restore, and unknown tokens
  stay unresolved for the user to assign.
- **Aliases are never reused** in a case; retired aliases keep restoring.
- **Joining** a case renumbers a document's clashing aliases to numbers no
  identity ever used, as one undo step.

## Invariants

- No originals in the session manifest or plaintext app data, and no
  plaintext key file or fallback.
- Tests use an injected store and never touch the real key or app data.
- Captured EN/RU replies from real assistants (`tests/fixtures/restore/`)
  restore completely.

## Consequences

Originals exist on disk for users who create cases. Encryption at rest
protects against other people and casual inspection, not against malware
running as the same user. Pseudonymized text remains personal data (GDPR);
the UI never calls a case or its output anonymous. The rules for cases,
their key and Restore are in the cases-and-privacy spec.

References: [full text before the docs doctor](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0041-cases-and-ai-response-restoration.md),
[interview](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/saved-mappings.md).
