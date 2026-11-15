# Cases and privacy

Folds: 0013, 0014, 0018, 0022, 0034, 0043, 0047, 0056, 0077, 0078, 0105.

Mappings, saved cases and their key, restoring AI replies, what never leaves
the device, and crash reports. The user-facing summary is
[privacy.md](../privacy.md); keep it true when a rule here changes. Terms are
in [CONTEXT.md](../../CONTEXT.md).

## Mappings

- Pseudonymization is the only replacement behaviour: no anonymization mode
  and no shared category markers.
- Aliases are stable category tokens (`PERSON_1`, `ORG_1`): a letter first, a
  letter or digit last, only ASCII letters, digits, `_` and `-`.
- Never offer realistic fake names, because they are harder to restore and
  collide with real names.
- Alias names are independent of identity IDs; merges and splits never
  renumber unrelated aliases.
- An alias is never reused, and a retired alias keeps restoring.
- Values of one category that are equal after case, spacing and
  typographic-quote normalization share an identity; the user can still split
  identical names by occurrence.
- Same alias means same entity; an owner ("Belongs to") is a separate
  relation with separate aliases.
- Apart from a unique propagated form, linking always needs a user action.
- Every occurrence keeps its exact original bytes; unrelated edits shift
  tracked occurrences, and editing or deleting an alias removes its
  provenance.
- Pasted lookalikes never gain provenance.
- A token already in the source is reserved; typing an existing alias offers
  linking, never an error.
- A category correction keeps custom aliases and replaces generated
  wrong-category tokens; cross-category linking moves only the moved
  mentions to the destination category.
- The primary wording is the longest; a saved or kept primary wins only
  among equally long ones.
- Apply is explicit and is one undoable edit; Keep changes no text.
- Keep original on an applied mention reverts it to the immediate
  pre-replacement text and records Keep; Undo takes it back to proposed.
- A stale revision or offset never replaces text; the operation is refused.
- Rescan needs no confirmation: it keeps every decision and manual addition
  and only adds new wordings.
- Cancelling a scan rejects late results and never reverts applied edits.
- Pending proposals never block copy or save, and copying never applies
  them.
- Copy Markdown copies the text exactly; Copy for AI prepends an alias-only
  note asking the AI to keep placeholders verbatim.
- Undo and redo restore text, provenance and Keep decisions together; review
  metadata follows the editor's undo units, never a second snapshot history.
- Typing never clones the identity index; undo journals share immutable
  policy and originals.
- 2 MiB with 20,000 occurrences is the stress benchmark, not a cap.

## Without a case

- A mapping, its originals and its provenance live only in the tab and its
  undo history, and end when the tab closes.
- Saved Markdown, sidecars, the session and copied output never carry
  originals.
- No mapping survives quit without a case.

## Cases

- A case is a named saved mapping; a document belongs to at most one.
- Creating a case (Save as Case, or opening a folder as a case) is the
  consent to store originals; the case then autosaves.
- A case is `mdoc/cases/<id>.case`: `MDOCCASE1`, a 12-byte nonce, then
  AES-256-GCM over JSON, authenticated with the case id.
- The key is a Keychain item on macOS, a DPAPI-protected `key.dpapi` on
  Windows, and a Secret Service item (`application` = `io.github.lawkitt.mdoc`,
  `kind` = `case-key`) over pure-Rust D-Bus on Linux.
- Without a keyring, cases are off with an explanation.
- Never write a plaintext key file or a plaintext fallback, not for dev
  builds and not as an opt-in.
- The session holds case IDs, order, names, collapse state and summaries
  (name, document count, colour), never member paths, names or originals.
- Joining (Move to case, or a drop on a case header) leaves any current case
  first and is one undo step in the joining tab.
- On join, a shared original takes the case's alias, a clashing alias takes
  the next number no identity ever used, and unmatched case identities are
  seeded as proposals.
- Join matching prefers the same alias, then a shared original in any
  category, so homonyms stay apart and a category change renames.
- A member's changed identities are pushed to other open members as an edit
  in each one's own history; there is no linked case-wide undo.
- Merges and splits are not pushed; members pick up the aliases on their
  next sync.
- A reopened member re-links current or earlier alias tokens as applied
  replacements.
- Deleting a case or forgetting an identity is confirmed and irreversible,
  and never touches Markdown files.
- Pseudonymize Clipboard replaces exact whole-word originals of the active
  case (or live mapping), longest first, with no detection.
- Restore Clipboard restores the active tab's aliases in the clipboard text.
- Clipboard actions never mix cases, because aliases repeat across cases.
- A case's Never and Always lists are shown on its page, remove only.
- Tests use an injected store and never touch the real key or app data.
- Encryption at rest protects against other people and casual inspection,
  not against malware running as the same user.
- Pseudonymized text is still personal data; never call a case or its output
  anonymous.

## Case key access

- Read the key only for an action that needs it: Save as Case (including
  from a close or quit prompt), opening a folder as a case, and Unlock.
- Moving a tab into a case needs a loaded case, so it never opens the key.
- Nothing asks at launch: launch reads the key with dialogs off, and any
  quiet error leaves cases locked.
- Never fall back to a key read with dialogs allowed without the explainer
  first.
- On macOS with dialogs off (`SecKeychainSetUserInteractionAllowed`),
  -25308, -25293 and -128 all mean "may ask", never "denied".
- The per-query `kSecUseAuthenticationUIFail` flag does not prevent the
  dialog; only the process-wide switch does.
- Before a read that may ask, the explainer (`src/workspace/keychain_ui.rs`)
  says macOS will ask for the login password, which mdoc never sees, and to
  choose Always Allow; Continue proceeds and Not now cancels the action.
- The explainer's animated preview sits below its text, captioned "After
  Continue, macOS shows this:", because above the text testers took it for
  the real dialog.
- The preview loops at up to 30 fps, shows one frame under Reduce Motion,
  and is announced as an image.
- Windows and Linux never show the explainer.
- While cases are locked, members keep their case link and the bell counts
  the "Saved cases are locked" notice with Unlock….
- The locked notice opens by itself only when a locked case is needed: a
  restored tab carries a case link, Paste AI Reply or Pseudonymize Clipboard
  finds no mapping, or the Keychain was declined after an action.
- A denial keeps members locked and keeps the action; Try again… finishes it
  once the key opens.
- A failed key read never drops members' case links.
- A gated action that arrives while one waits runs after it; a repeated
  Save as Case on a saved tab does nothing.
- The build identity is decided once at runtime (`key::keychain()`): team
  F33DLM8K53 with an embedded provisioning profile is Protected (data
  protection keychain), the team without a profile is Release (login
  keychain, service `io.github.lawkitt.mdoc`), anything else is Dev.
- A Protected build whose profile lacks the entitlement behaves as Release.
- A Dev build keeps its key under `io.github.lawkitt.mdoc.dev` and its cases
  in `mdoc/cases-dev/`, because a dev key must never later prompt the
  release.
- Only cases are kept apart for Dev builds; settings, the session and models
  are shared, and session case members a Dev build cannot find are missing,
  not locked.
- A Protected build with no protected key migrates on the next Unlock: one
  dialog, the key is copied to the data protection keychain, and the login
  item is deleted with dialogs off or left unread if deleting would ask.
- `scripts/keychain-launch-check.sh` runs on a clean macOS user account with
  the signed release before every release that touches the case key.

## Case page

- The case page opens in the content card from the case header, Home's Cases
  list, the Share panel's Case row, the palette and File → Open Case….
- It is transient like Home: not a tab, not restored at launch, replaced by
  the next tab activation.
- Opening the case page never reads the case key or a member file; closed
  members show "—" for mention counts.
- A locked case's page shows only its name, document count and Unlock….
- Documents list every member as open, closed or missing; Remove from case
  is the only action on a missing file, and Open all opens every member.
- Names group the case's identities by category, with Forget identity and
  Add a name; wording, category and merges are edited in a document's
  Replacements panel.
- A case's colour is one of eight swatches (Red, Orange, Amber, Green, Teal,
  Blue, Violet, Pink), each passing the contrast tests in both themes.
- A case without a stored colour snaps its derived hue to the nearest
  swatch.
- A case colour marks the case, never a state.
- After Delete case and forget originals…, the page closes to Home and
  member tabs stay open without a case.

## Restore

- Restore replaces each alias with its identity's primary wording in one
  undoable edit.
- Matching ignores case and `_`/`-`/space separators at word boundaries,
  keeps Markdown wrappers and splits attached suffixes.
- A loose form (`person 1`) restores only when it names a known alias.
- A partial match never restores, Restore never guesses, and Russian
  inflection is never added.
- An unknown alias-shaped token is shown as unresolved and the user can
  assign it.
- Restoring prefers an identity's current alias over another's retired one.
- Paste AI Reply opens a restore tab seeded with the source's identities;
  restored values are proposals that keep their aliases, so Apply
  re-pseudonymizes and Undo returns the reply as pasted.
- The Share panel's reply box restores as the user pastes or types, while
  aliases are applied or a reply exists.
- The reply box keeps exactly what was pasted or typed: no Markdown paste or
  Enter rules.
- The Reply view is read-only and follows mapping changes live.
- The summary counts distinct identities restored and distinct unknown
  tokens; Next unrecognized visits every occurrence.
- A document has one reply: stored encrypted in its case file, or kept only
  while the tab is open without a case.
- An untitled tab in a case keeps its reply in memory until its first save.
- Clearing the box removes the reply, from the case too.
- The reply is kept in alias form, never as restored text.
- When a document's aliases change under a reply (a join, a case-wide
  rename, or undo of either), the reply's tokens are rewritten to the
  aliases their identities hold now.
- The reply's restored text never reaches the session or plaintext app
  data.
- On mobile, `pasted_markdown` in `mdoc-markdown` chooses between a copy's
  HTML and plain text; the apps only read the pasteboard or share.
- Plain text that shows a Markdown heading, table separator or bold is the
  source and stays byte for byte; a list does not count, and HTML of only
  paragraphs leaves the plain text.
- HTML conversion never escapes `_`, so every alias survives; other Markdown
  characters are escaped.
- HTML conversion keeps an unknown tag's text, drops `head`, `style` and
  `script`, and keeps a link target only for `http`, `https` and `mailto`.
- HTML is tokenized with `html5ever`; the conversion is mdoc's own, because
  of the alias rule.
- The desktop reply box stays plain text.

## Restore fixtures

- `tests/fixtures/restore/<chatgpt|claude>-<en|ru>/` holds `sent.md` (Copy
  for AI output), `identities.json`, `reply.md` (as copied) and
  `expected.md`; `README.md` records model, date, prompt and capture steps.
- Every capture restores with no unresolved token, no current or previous
  alias left, and text equal to `expected.md` byte for byte, in the restore
  tab and the Reply view.
- Pseudonymizing the demo with `identities.json` must give `sent.md` without
  its note.
- Replies are never hand-edited; each `expected.md` is written by hand,
  independently of Restore.
- Captures are frozen: never regenerated, and a new one is added beside them
  with a date suffix.
- A paraphrase that carries no token ("the Employee") is out of Restore's
  scope.
- Widen the matcher only for a shape that can only be an mdoc alias.
- Synthetic edge shapes live in `crates/mdoc-pii/fixtures/restore/`.
- Demo documents and fixtures use fictional data only, with emails at the
  reserved `example.com`.

## What never leaves the device

- Detection, conversion, recognition and cases run on the device.
- mdoc goes online only for the update check, a download the user chose,
  and a page the user opens in the browser.
- The update check is one unauthenticated GET to the GitHub Releases API
  with an mdoc User-Agent; it sends nothing but what the request itself
  carries.
- Turning automatic checks off stops every automatic request.
- There is no telemetry and no server endpoint.
- Recents store paths only, never content, and can be cleared.
- The session holds ordered paths, the active tab and view metadata, never
  unsaved text, undo, cached pages, temporary DOCX paths or originals.
- Decision history and the menu bar shelf never write document text to
  disk.

## Crash reports

- A panic hook writes `mdoc/crash-report.txt` in app data, in app mode only
  (not the DOCX worker or self-test); only the latest report is kept, and the
  default hook then runs.
- mdoc keeps no log file.
- A report holds version, OS and architecture, thread, panic location, the
  first mdoc frame, the backtrace, a warning if the binary changed after
  launch, and a window-state line of words and numbers.
- A report never holds document text, file names or clipboard contents.
- Absolute paths lose their folders; only Rust source file names stay, and
  other file names become `<file>`.
- The panic message is never kept; known standard-library panics get a fixed
  label.
- A string-slicing panic is parsed strictly and kept only as numbers;
  anything that does not parse exactly is withheld.
- Masking is best effort, so the user always sees the exact text before it
  leaves.
- The next launch offers See what would be sent and Dismiss; the preview
  offers Send… (a prefilled GitHub issue, cut to about 6,000 characters) and
  Copy, and each of them deletes the report.
- Nothing is sent automatically.
