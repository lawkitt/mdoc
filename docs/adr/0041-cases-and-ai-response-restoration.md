# Cases and AI-response restoration

Status: Accepted design, 2026-10-11; phases 1–2 implemented the same day and
phase 3 on 2026-10-12, phase 4 on 2026-10-13. See the [design interview](../design/saved-mappings.md) for all 27 decisions and the
prior-art review. From phase 2, supersedes the "mappings and originals are never
serialized" rule of [ADR 0002](0002-reviewable-local-pseudonymization.md),
[ADR 0014](0014-highlighted-pii-replacements.md) and
[ADR 0018](0018-document-local-identity-review.md) for cases only. Opens the
return journey that [ADR 0016](0016-pii-production-qualification-scope.md) deferred.

## Problem

Users send pseudonymized Markdown to ChatGPT or Claude and get back text full of
`PERSON_1`-style aliases. mdoc cannot put the real values back, and a mapping
dies with its tab, so the same client gets different aliases in every document.
Comparable tools (Anoni, RedactLocal, Docuflair) all restore from a local
encrypted map, and the better ones keep one map per case.

## Decision

- **Restore.** Paste AI Response (toolbar, File menu, ⌥⇧⌘V; later the case
  menu and opening a .md into a case) creates a new tab whose aliases are
  replaced by each identity's **primary wording** in one undoable edit. The new
  tab is seeded with the source's identities, so restored values are
  **proposals that keep their aliases**: Apply re-pseudonymizes them for the
  next round, and Undo returns to the reply as pasted. Matching is lenient on
  case and `_`/`-`/space separators at word boundaries, keeps Markdown wrappers
  and splits attached suffixes. Loose forms (`person 1`) count only when they
  name a known alias. Partial matches never restore. Unknown tokens are highlighted as **unresolved** and
  can be assigned to an identity; nothing is guessed. No automatic RU inflection.
- **Copy for AI.** A second copy action, shown when aliases are applied,
  prepends an alias-only note asking the AI to keep placeholders verbatim.
  Copy Markdown still copies the text exactly. This supersedes ADR 0019's
  single copy action.
- **Case.** A named saved mapping, shown as a tab group in the sidebar. One case
  per document; ungrouped tabs keep memory-only mappings. Creating a case is the
  explicit consent to store originals; afterwards changes autosave.
- **Storage.** One AES-256-GCM file per case in app data, key in macOS Keychain /
  Windows DPAPI / Linux Secret Service (else cases are disabled). No plaintext
  fallback, no extra unlock in v1, no export yet. The session manifest stores only
  case IDs, order, names and collapse state.
- **Consistency.** Joining a case proposes known originals with case aliases and
  renumbers the document's own identities in one undoable step. Identity-level
  edits apply case-wide to open member tabs. Aliases are never reused; retired
  aliases keep restoring, and reopened member files re-link alias tokens.
- **Iterating.** Restored tabs are case members (phase 2); their restored values
  are proposals, so Apply then Copy for AI sends the next round.
- **Forgetting.** Deleting a case or forgetting an identity is confirmed and
  irreversible; Markdown files are untouched.

## Delivery

1. Memory-only restore (no storage change; this ADR's supersession not yet in effect).
2. Saved, encrypted single-document cases; reopen re-linking.
3. Multi-document cases and case-wide edits.
4. Folder drop as a case; clipboard shortcut and other roadmap step 5 items.

Phase 1 ships only after CI fixtures of real EN/RU AI replies restore as expected.
The fixtures in `crates/mdoc-pii/fixtures/restore/` are synthetic replies
modelled on typical assistant output; replies captured from ChatGPT and Claude
still need adding before release.
Phase 2 requires encryption, tamper and leak checks (no originals in the
manifest or plaintext app data).

## Phase 2 as implemented

- `src/cases.rs` stores one `<id>.case` file per case in `mdoc/cases/`:
  `MDOCCASE1`, a 12-byte nonce, then AES-256-GCM over JSON, authenticated with
  the case id. The key is a Keychain generic password on macOS
  (`io.github.lawkitt.mdoc` / `case-key`) and a DPAPI-protected `key.dpapi` on
  Windows. On Linux the key is a Secret Service item (`application` =
  `io.github.lawkitt.mdoc`, `kind` = `case-key`) in the default collection,
  reached over pure-Rust D-Bus (`secret-service`/zbus); without a running
  keyring, cases stay off with an explanation (no plaintext fallback).
- A case is created from the tab context menu (**Save mapping as case**), named
  after the document; it is renamed inline from the context menu or by
  double-clicking its sidebar header. No naming dialog.
- Reopening a member seeds its review with the case's identities and re-links
  exact current or earlier alias tokens as applied replacements.
- The unsaved-mapping question is asked on tab close, not on quit, and can be
  turned off in Settings → Saved cases. Closed cases are reached from
  **File → Open Case…**; the empty page does not list them yet.
- Tests use an injected store and never touch the real key or app data.

## Phase 3 as implemented

- A tab joins a case from **Move to case “…”** in its context menu (the five
  most recent cases) or by dropping it on a case header; joining another case
  leaves the current one first. `mdoc_pii::restore::plan_join` lines it up:
  a shared original takes the case alias, a clashing alias the next number no
  identity ever used, and unmatched case identities are seeded as proposals.
  The retagging is one undo step in the joining tab.
- After a member's mapping changes, identities whose alias or category changed
  are pushed to the other open members as an edit in each one's own history;
  case identities a member lacks are seeded there. Undo in the originating tab
  is an ordinary change that propagates again; there is no linked, blockable
  case-wide undo. Merges and splits are not propagated yet; members pick up the
  resulting aliases on their next sync.
- Case matching prefers the same alias, then a shared original in any category,
  so homonyms stay apart and a category change renames rather than duplicates.
- The replacements panel footer folds **In “case”, not in this document (n)**:
  those names with their aliases, and a field adding a name to the case
  (category guessed; reclassify it in a member). Case headers fold their
  member rows (the active tab stays visible); folding is kept in the session.

## Phase 4 as implemented

- A dropped folder opens its own supported files, sorted by name; subfolders
  and hidden files are skipped and counted. An empty case named after the folder
  is created at once and its tabs carry it as a pending membership: each joins
  when its Markdown loads or its conversion finishes.
- **Pseudonymize Clipboard** (File menu, ⌥⇧⌘C, mdoc focused) runs
  `RestoreMap::pseudonymize`: exact whole-word originals of the active tab's
  case (or its live mapping), longest first, no detection. A system-wide hotkey
  needs platform code and is deferred.
- **Before / after** replaces the editor with two read-only `mdoc-editor`
  instances, original and pseudonymized, formatted like the editor, replaced
  names highlighted, scrolling together by fraction of height. It is a snapshot
  taken when opened. The editor's read-only mode blocks every user edit path
  while the host can still set the text.
- A case stores category tokens its scans skip; member tabs filter scan
  results before ingesting. Manual additions are unaffected. The empty page
  still does not list cases.

## Consequences

Originals now exist on disk for users who create cases. Encryption at rest
protects against other people and casual inspection, not against malware running
as the same user. Pseudonymized text remains personal data (GDPR); the UI must not
call a case or its output anonymous. Multi-tab undo becomes more complex in phase 3.
