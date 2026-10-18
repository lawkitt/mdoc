# Cases and AI-response restoration

Status: Accepted, 2026-10-11. Supersedes, for cases only, the "mappings and
originals are never serialized" rule of ADRs 0002, 0014 and 0018, and ADR 0019's
single copy action. Amended by [ADR 0042](0042-live-before-after-view.md) (live
view), [ADR 0043](0043-deferred-keychain-access.md) and
[ADR 0045](0045-prompt-free-case-key.md) (key access),
[ADR 0048](0048-workspace-shell-redesign.md) (sidebar and header placement), and
[ADR 0056](0056-captured-ai-reply-fixtures.md) (captured replies).

## Problem

Users send pseudonymized Markdown to ChatGPT or Claude and get back text full of
`PERSON_1`-style aliases. mdoc could not put the real values back, and a mapping
died with its tab, so the same client got different aliases in every document.
Comparable tools (Anoni, RedactLocal, Docuflair) restore from a local encrypted
map, the better ones one map per case.

## Decision

- **Restore.** Paste AI reply (File menu, ⌥⇧⌘V, case actions) creates a new tab
  whose aliases are replaced by each identity's **primary wording** in one
  undoable edit. The tab is seeded with the source's identities, so restored
  values are **proposals that keep their aliases**: Apply re-pseudonymizes them
  for the next round, Undo returns to the reply as pasted. Matching is lenient
  on case and `_`/`-`/space separators at word boundaries, keeps Markdown
  wrappers and splits attached suffixes. Loose forms (`person 1`) count only
  when they name a known alias. Partial matches never restore. Unknown tokens
  are highlighted as **unresolved** and can be assigned; nothing is guessed. No
  automatic RU inflection.
- **Copy for AI** prepends an alias-only note asking the AI to keep placeholders
  verbatim. Copy Markdown still copies the text exactly.
- **Case.** A named saved mapping, shown as a sidebar section. One case per
  document; tabs without a case keep memory-only mappings. Creating a case
  (**Save mapping as case**, named after the document, renamed inline) is the
  consent to store originals; changes then autosave. The unsaved-mapping
  question is asked on tab close, not on quit, and can be turned off in
  Settings → Saved cases. Closed cases open from **File → Open Case…**.
- **Storage.** One `<id>.case` file per case in `mdoc/cases/`: `MDOCCASE1`, a
  12-byte nonce, then AES-256-GCM over JSON, authenticated with the case id. The
  key is a Keychain item on macOS (ADR 0045), a DPAPI-protected `key.dpapi` on
  Windows, and a Secret Service item (`application` = `io.github.lawkitt.mdoc`,
  `kind` = `case-key`, pure-Rust D-Bus) on Linux. Without a keyring, cases are
  off with an explanation. No plaintext fallback, no export yet. The session
  manifest stores only case IDs, order, names and collapse state.
- **Joining.** Move to case (context menu, five most recent) or a drop on a case
  header; joining another case leaves the current one first.
  `mdoc_pii::restore::plan_join` lines it up: a shared original takes the case
  alias, a clashing alias the next number no identity ever used, unmatched case
  identities are seeded as proposals. One undo step in the joining tab. Matching
  prefers the same alias, then a shared original in any category, so homonyms
  stay apart and a category change renames rather than duplicates.
- **Case-wide edits.** After a member's mapping changes, changed identities are
  pushed to other open members as an edit in each one's own history; missing
  case identities are seeded. Undo is an ordinary change that propagates again;
  there is no linked case-wide undo. Merges and splits are not propagated;
  members pick up the aliases on their next sync. Aliases are never reused;
  retired aliases keep restoring; reopened members re-link current or earlier
  alias tokens as applied replacements.
- **Panel footer** folds **In “case”, not in this document (n)**: those names
  with aliases, and a field adding a name to the case; with none it reads
  "Add a name to “case”" (ADR 0046).
- **Folder drop** opens the folder's supported files sorted by name (subfolders
  and hidden files skipped and counted) and creates a case named after the
  folder; each tab joins when its Markdown loads or conversion finishes.
- **Pseudonymize Clipboard** (File menu, ⌥⇧⌘C, mdoc focused) replaces exact
  whole-word originals of the active case (or live mapping), longest first,
  without detection. A system-wide hotkey is deferred.
- **Before / after** shows original and pseudonymized text in two read-only
  editors (ADR 0042).
- **Categories to detect** are stored per case; member tabs filter scan results
  before ingesting. Manual additions are unaffected.
- **Forgetting.** Deleting a case or forgetting an identity is confirmed and
  irreversible; Markdown files are untouched.

## Invariants

- No originals in the session manifest or plaintext app data.
- Tests use an injected store and never touch the real key or app data.
- Captured EN/RU replies from real assistants (`tests/fixtures/restore/`,
  [ADR 0056](0056-captured-ai-reply-fixtures.md)) restore completely; the
  fixtures in `crates/mdoc-pii/fixtures/restore/` are synthetic edge shapes.

## Consequences

Originals exist on disk for users who create cases. Encryption at rest protects
against other people and casual inspection, not against malware running as the
same user. Pseudonymized text remains personal data (GDPR); the UI never calls a
case or its output anonymous.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0041-cases-and-ai-response-restoration.md),
[interview](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/design/saved-mappings.md).
