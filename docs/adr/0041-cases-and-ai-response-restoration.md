# Cases and AI-response restoration

Status: Accepted design, 2026-10-11; phase 1 implemented the same day, phases
2–4 not yet. See the [design interview](../design/saved-mappings.md) for all 27 decisions and the
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

## Consequences

Originals now exist on disk for users who create cases. Encryption at rest
protects against other people and casual inspection, not against malware running
as the same user. Pseudonymized text remains personal data (GDPR); the UI must not
call a case or its output anonymous. Multi-tab undo becomes more complex in phase 3.
