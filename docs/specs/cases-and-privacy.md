# Cases and privacy

Mappings, saved cases and their key, restoring AI replies, what never leaves
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

## Cases

- Creating a case (Save as Case, or opening a folder as a case) is the
  consent to store originals; without one, no mapping outlives its tab.
- Never write a plaintext key file or a plaintext fallback, not for dev
  builds and not as an opt-in; without a keyring, cases are off.
- There is no case-wide undo: a change reaches other open members as an edit
  in each one's own history.
- Encryption at rest protects against other people and casual inspection,
  not against malware running as the same user.
- Pseudonymized text is still personal data; never call a case or its output
  anonymous.
- A case colour marks the case, never a state.

## Case key access

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

- Detection, conversion, recognition and cases run on the device.
- mdoc goes online only for the update check, a download the user chose,
  and a page the user opens in the browser.
- The update check is one unauthenticated GET to the GitHub Releases API
  with an mdoc User-Agent; it sends nothing but what the request itself
  carries.
- There is no telemetry and no server endpoint.
- Recents store paths only, never content; decision history and the menu
  bar shelf never write document text to disk.

## Crash reports

- mdoc keeps no log file.
- Nothing is sent automatically: the next launch shows the exact report
  before Send… or Copy, because masking document text, file names and
  clipboard contents out of it is best effort.
