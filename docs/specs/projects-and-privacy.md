# Projects and privacy

Mappings, the Vault and its key, restoring AI replies, what never leaves
the device, and crash reports. The user-facing summary is
[privacy.md](../privacy.md); keep it true when a rule here changes. Terms are
in [CONTEXT.md](../../CONTEXT.md).

## Mappings

- Pseudonymization is the only replacement behaviour: no anonymization mode
  and no shared category markers.
- Never offer realistic fake names, because they are harder to restore and
  collide with real names.
- Detection never changes text: applying proposals is explicit and
  undoable, and linking two wordings needs a user action unless the form is
  a unique propagated one.
- 2 MiB with 20,000 occurrences is the stress benchmark, not a cap.

## Projects and Vault

- Projects, Vault documents, AI replies, Source Originals and the Vault's
  index are encrypted at rest with `AES-256-GCM` under the OS-held key, under
  opaque names ([ADR 0125](../adr/0125-unified-encrypted-vault-and-projects.md));
  password-protected originals stay locked on disk.
- Never write a plaintext key file or a plaintext fallback, not as an
  opt-in and not for a dev build outside a debug build's `MDOC_DEV_PROFILE`
  scratch folder; without a keyring, the Vault is unavailable.
- The key is the only way to read the Vault, so desktop shows a recovery key,
  accepts one on the wrong-key screen, and exports a Project as Markdown and
  originals. Mobile offers neither: its files never reach a backup, and device
  transfer is its way to a new phone.
- The recovery key shows only behind Settings › Vault › Show Recovery Key,
  with no Copy button, and never reaches a log, a notice or a plain file.
- An accepted recovery key goes to its own slot in the OS key store
  (`vault-key`), read before the old key, which is never rewritten.
- mdoc keeps every import, with its original and mapping, until the user
  deletes it; [privacy.md](../privacy.md) says so in those words.
- A build that introduces the Vault leaves the old cases folder and key
  untouched.
- The Vault folder sits beside the key's folder, never inside it (`vault`
  beside `cases`, `vault-dev` beside `cases-dev`), so a dev build never opens
  the release Vault.
- Text leaves (Copy for AI, Share, shelf, MCP) only after a fresh check
  against the Project's names; a stored copy of AI-ready text is never served.
- A selection the user copies by hand is their own copy, on every platform:
  never blocked, with no note for the AI, and marked as a restored reply's
  copy is unless the core finds it clean
  ([#1209](https://github.com/lawkitt/mdoc/issues/1209)). "Excluded from AI"
  does not stop it.
- A saved AI reply restores by the identities its aliases meant when saved,
  whatever a later move or merge did to the aliases.
- Mobile decrypts an original for viewing into a cache wiped on close and at
  launch. On iOS, encryption at rest adds little beyond Data Protection and
  exists to keep one storage path; do not present it as extra security.
- Desktop `session.json` stores opaque Vault ids only, never Project or
  Document titles.
- Moving a document into a Project reconciles matching originals to the target
  Project's aliases and updates the document text undoably; deleting a non-empty
  Project always asks whether to move documents to Unfiled or delete them.
- Deleting a Project deletes its own AI replies whichever way its documents
  go; a document's replies go with the document.
- There is no project-wide undo: a change reaches other open members as an edit
  in each one's own history.
- Encryption at rest protects against other people and casual inspection,
  not against malware running as the same user.
- Pseudonymized text is still personal data; never call a Project or its output
  anonymous.
- A Document's notes are private: no Copy for AI, Share, shelf or connector
  read sends them. Only Export Project writes them, as `<Title>.notes.md`
  beside the Markdown.
- Promote to Document copies a saved reply, restored with its place's mapping
  as it is then, into a new unreviewed Document with no original; the reply
  stays.
- A Project colour marks the Project, never a state.

## Vault key access

- The per-query `kSecUseAuthenticationUIFail` flag does not prevent the
  macOS password dialog; only the process-wide
  `SecKeychainSetUserInteractionAllowed` switch does.
- The explainer's animated preview sits below its text, captioned "After
  Continue, macOS shows this:", because above the text testers took it for
  the real dialog.

## Restore

- A paraphrase that carries no token ("the Employee") is out of Restore's
  scope; widen the matcher only for a shape that can only be an mdoc alias.
- The HTML-to-Markdown conversion for phone pastes is mdoc's own, not a
  library's, because it must never escape `_` in an alias.
- Demo documents and fixtures use fictional data only, with emails at the
  reserved `example.com`.

## What never leaves the device

- Detection, conversion, recognition, and the Vault run on the device.
- mdoc goes online only for the update check, a model download the user chose,
  and a page the user opens in the browser.
- A device transfer ([ADR 0127](../adr/0127-cross-device-p2p-vault-transfer.md))
  sends a bundle sealed under a one-time pairing key, over a direct connection
  on the local network or as a file the user moves; it contacts no relay or
  other third party.
- A transfer never rewrites aliases: a Project whose aliases clash with the
  local one arrives as a separate copy.
- The desktop MCP connector ([ADR 0126](../adr/0126-local-mcp-ai-connector.md))
  serves one AI project, chosen and always shown in mdoc, plus the Personal
  documents the user allowed: only `Ready` documents, labelled from their
  reviewed text and never by stored title. Unfiled documents stay hidden.
  `deliver_reply` restores only inside mdoc, returns no real name, and is
  refused unless it names a document it read from what is served now.
- A Project's AI activity records each connector read that handed text out
  and each delivery, with its app and time: ids only, never text, a title
  or a name. A read whose record can't be written hands nothing out; the
  activity goes when its Project does.
- A system notification for a delivery or a review request names only the
  AI app and the document's AI label; its tag holds ids. mdoc takes focus
  only when the user clicks it, or with "Open deliveries when they arrive"
  on.
- A Personal alias stands for one stored value. mdoc puts it back exactly as
  stored and computes nothing from it
  ([ADR 0129](../adr/0129-personal-documents-restore-only-aliases.md)).
  Personal is closed to an AI until the user allows chosen documents or all.
- Any process running as the user can read the AI project's pseudonymized
  documents through the connector; never claim otherwise.
- The update check is one unauthenticated GET to the GitHub Releases API
  with an mdoc User-Agent; it sends nothing but what the request itself
  carries.
- There is no telemetry and no server endpoint.
- Decision history and the menu bar shelf never write document text to
  disk.

## Crash reports

- mdoc keeps no log file.
- Nothing is sent automatically: the next launch shows the exact report
  before Send… or Copy, because masking document text, file names and
  clipboard contents out of it is best effort.
