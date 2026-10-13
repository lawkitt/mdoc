# Saved mappings and the AI return journey — design interview

Status: grilling in progress, started 2026-10-11. Nothing here is accepted until
the frontier is empty and the user confirms shared understanding; accepted
decisions then move into an ADR.

## Request

Users save mappings of aliases → original values, then apply a saved mapping to
new Markdown they bring back after working with it in an external AI tool.
Future idea: grouped tabs that share one mapping.

## Starting point (current behavior)

- Mappings, originals and provenance live only in the open tab and its undo
  history; Save, Copy, session restore and closing never serialize them
  ([ADR 0002](../adr/0002-reviewable-local-pseudonymization.md),
  [ADR 0014](../adr/0014-highlighted-pii-replacements.md),
  [ADR 0018](../adr/0018-document-local-identity-review.md)).
- The roadmap defers exactly this cluster: AI-response import and restoration,
  cross-document identities, persistent/exported replacement maps, named tab
  groups ([ROADMAP](../../ROADMAP.md#deferred); [ADR 0016](../adr/0016-pii-production-qualification-scope.md)).
- Model: `Identity` (alias, category, owner, custom flag) owns `Variant`s (exact
  original wordings) whose `mentions` are tracked occurrences. Aliases are ASCII
  tokens like `PERSON_1`, `ORG_2`, `REDACTED_1`.
- Session manifest stores paths and view metadata only, never text.

This feature therefore reverses a standing invariant ("originals are never
written to disk") and opens the deferred return journey. Each reversal needs an
explicit decision below.

## Prior art (web research, 2026-10-11; vendor claims unverified)

| Product | Placeholders | Mapping storage / scope | Restore |
|---|---|---|---|
| [Anoni](https://anoni.dev/en/guides/anonymize-before-ai) | Realistic fake names | Local AES-256-GCM vault | Paste AI reply back at Export step |
| [RedactLocal](https://www.redactlocal.com/) | `Person_1` tokens | Local encrypted vault | Restore from vault |
| [anonym.plus](https://anonym.plus/blog/how-to-anonymize-before-ai.html) | One-way or reversible (encrypted) | User-held key | Deanonymize tab |
| [messymatch](https://messymatch.com/anonymize-pdf-for-chatgpt) | Tokens, in browser | Mapping file the user keeps | Reverse with mapping file |
| [Docuflair Mask](https://www.docuflair.com/en/pages/products/pseudonymization.html) | Consistent across a document *set* | Separate DB; exporting the map is its own permission; audit trail | One-click; selective per-entry restore |
| [local-privacy-gateway](https://github.com/PLUCNT/local-privacy-gateway) | `<PERSON_001>` | SQLite, AES-GCM, key file 0600; named sessions | Lenient: tolerates lowercase/quotes/backticks; unknown tokens left and reported; tells the model to keep placeholders verbatim |
| [LLM Guard Vault](https://llm-guard.com/output_scanners/deanonymize/) | Tokens | In-process vault shared by anonymize/deanonymize | Exact placeholder lookup |
| [Anonproxy](https://github.com/jfreemansh/Anonproxy) | Tokens | Per-engagement isolated SQLite vault | On response path |

Recurring lessons: the map is the most sensitive artifact (everyone encrypts it
or keeps it apart from the documents); scope maps per engagement/matter so one
client's tokens never decode with another's; restore leniently but report
unknown/invented tokens; prompt the model to keep placeholders verbatim; nobody
documents inflection-aware restoration (a gap for RU); pseudonymized data is
still personal data under GDPR, so never call the result anonymous.

## Anoni 0.81.0 hands-on (macOS, 2026-10-11)

Flow is a three-step stepper per document: **Drop → Check → Export**.

- **Check**: right panel lists detections grouped by category with counts
  (`ORGANIZATION 7 (20)` = values (mentions)), checkbox per value, filter, "Mask
  another term…", Re-analyze, and a "Re-identification: Not assessed" indicator.
  Tokens render as coloured mono chips `PLACE·1` (middle dot). Detection is
  coarse: "New York ☒ United Kingdom" became one PLACE; "Lender"/"Borrower"
  flagged as organizations; on a Russian protocol it found only 2 items
  (a date and a contract number as ACCOUNT) — names/dates in words missed.
- **Export**: Pseudonymize (Tokens | Realistic) or Redact (irreversible black
  bars). Outputs: Pseudonymized Word (keeps DOCX formatting), PDF, Save…,
  Copy for AI, Compliance report. View switch: Before / After / Side by side /
  Reply, with zoom.
- **Restore** happens *inside the same document*: an "AI reply" box; the
  restored text appears in a Reply view with category-coloured chips, plus
  "N values restored · M unrecognised token", Copy the reply, Save the reply.
  Tested: `ORGANIZATION 1`, `organization·1`, `ORGANIZATION_1`, `DATE-1`,
  `**PLACE·2**'s` all restored; invented `PERSON·9` was counted but **not
  highlighted** in the reply — easy to miss.
- **Key**: per-document vault, decodes only on this computer unless the user
  shares a generated passphrase + "Save the vault…" file (or chooses their own
  passphrase). Standalone **Decode** tool: AI reply (file or paste) + key file
  + passphrase.
- **Cases** (Pro): "one pseudonym across a whole case; a name keeps the same
  pseudonym from one document to the next." Sidebar section, not tab groups.
- **Profiles** (workspace switcher): each has its own detection settings and
  **personal dictionary** ("terms always anonymized: client names, projects,
  internal references"); profiles from templates.
- Settings: default mode, pseudonym style, 15 toggleable categories (incl.
  SECRET, AGE, SENSITIVE, SOCIAL HANDLE, URL), CPU/auto acceleration, models
  (CamemBERT FR + General PII), global shortcut **⌘⇧A anonymizes the clipboard
  in any app**, "Connect an AI", local statistics, settings backup (plain file),
  reset.
- Other tools: Watermark (templates, legibility, per-recipient coded copies in
  Pro), PDF → Word, Journal (Pro, timestamped audit log exportable as a
  register), Watched folders (Pro, auto-redact into an output folder).

Where mdoc is already stronger: editable Markdown with history, OCR, original
preview, identity variants/linking/ownership, scoped undo, EN/RU detection
with checksum rules. Where Anoni leads: return journey, cases, format-preserving
export, clipboard shortcut, before/after views, dictionary, audit/compliance.

## Decisions

### Round 1 (confirmed 2026-10-11)

1. **Direction.** v1 restores aliases → originals in AI output. Storage is
   designed so a later *reuse* direction (apply saved originals → same aliases
   in a new source document) needs no migration; reuse lands with shared maps.
2. **Persistence.** Saved mappings are written only on explicit user action, to
   app data, encrypted with an OS-held key (macOS Keychain / Windows DPAPI).
   Users can delete them. Export/sharing is a separate future decision. Closing
   a tab with an unsaved mapping warns once (can be turned off). This deliberately
   supersedes the "never serialized" invariant of ADRs 0002/0014/0018 for
   saved mappings only; unsaved live mappings keep current behavior.
3. **Unit.** A saved mapping is a named, standalone object (name, created date)
   that documents link to; a document links to at most one. v1 creates one
   from a single document's identities. UI noun: "Mapping".
4. **Restore entry.** "Paste AI response" (and opening a .md into a mapping)
   creates a new tab "Restored – <mapping name>" linked to the mapping. Each
   restored value is an applied, inspectable, undoable replacement (ADR 0014
   provenance). Copy/Save unchanged.
5. **Matching.** Auto-restore exact tokens and case / `_`·`-`·space separator
   variants at word boundaries, preserving Markdown wrappers; split attached
   suffixes (`'s`, `-у`). Unknown/invented tokens are highlighted unresolved;
   summary "Restored N · M unresolved". Partial matches never restore.
6. **Wording.** Each identity has a primary wording (default longest full form,
   user-changeable) used for restoration. A restored value's popup offers other
   known wordings and free edit. No automatic RU inflection in v1 (known limit).
7. **Placeholder notice.** Copy for AI gains an optional instruction to keep
   placeholders verbatim (aliases only), default on when linked to a mapping.

### Round 2 (confirmed 2026-10-11)

8. **Group = case.** A tab group is the sidebar form of a saved mapping; all
   member tabs share its identities. Saving a document's mapping turns that tab
   into a one-document case. Ungrouped tabs keep document-local mappings.
   Collapse/colour are presentation only. (Refines round 1 #3: the "Mapping"
   object is presented as a case.)
9. **Naming.** UI noun **Case**; "mapping" stays the term for what a case
   stores (originals ↔ aliases). Avoid "matter".
10. **Joining.** Case originals found in a joining document become proposals
    with the case's aliases (reuse direction). The document's own identities
    take the next free case numbers; already-applied aliases are renumbered in
    one undoable step with a summary. One case per document; leaving keeps text.
11. **Always-replace terms.** Case identities act as always-replace terms; "Add
    term to case…" adds names not yet seen. A cross-case dictionary is deferred.
12. **Roadmap from Anoni review.** Add: clipboard shortcut (active case),
    before/after/side-by-side view, category toggles per case, format-preserving
    DOCX/PDF export (deferred), audit journal + compliance report (deferred).
    Defer: re-identification risk hint, case key sharing. Rejected: realistic
    fake names (restoration/collision risk; ADR 0018 neutral aliases),
    irreversible Redact mode (ADR 0022), watermark and watched folders (outside
    the AI-preparation focus).

### Round 3 (confirmed 2026-10-11)

13. **Membership.** Tab context menu "New case…" (named; saves that tab's
    mapping); drag a tab into a case or "Move to case → …"; "+ New tab in case"
    row under the case's last tab. Dropping a folder as a case follows soon after.
    Collapsed case: name, colour dot, tab count; collapsed rail shows a coloured
    bracket.
14. **Autosave.** Creating a case is the explicit consent; afterwards mapping
    changes in member tabs autosave (debounced) to the encrypted store, and undo
    in a tab updates the case. Ungrouped tabs stay memory-only with the
    one-time close warning.
15. **Lifecycle.** Closing all of a case's tabs keeps the case. Closed cases
    appear in File → Open Case… and the empty page's recents; reopening restores
    member tabs by path. "Delete case and forget its originals" is separate,
    confirmed and irreversible; it wipes the mapping, not the Markdown files.
16. **Iterative turns.** A restored tab is a full case member. Its restored
    values are tracked, so Copy for AI re-pseudonymizes them; newly typed case
    originals are matched exactly; new names need a scan or Replace.
17. **Encryption.** One AES-256-GCM file per case in app data; key in macOS
    Keychain / Windows DPAPI; Linux uses Secret Service when available, else
    cases are disabled with an explanation. No plaintext fallback.
18. **No extra unlock in v1.** Optional Touch ID / Windows Hello gate deferred.
19. **Session manifest** stores per tab only the case ID and order, plus case
    name and collapsed state; never originals (ADR 0003 holds). Mapping is
    decrypted when a member tab activates; a missing or corrupt case opens the
    tab ungrouped with a notice.

### Round 4 (confirmed 2026-10-11)

20. **Restore entry points.** Case context menu, toolbar in case tabs, and
    Edit → Paste AI Response (⌥⇧⌘V); also File → open a .md "into case…".
    Works from an ungrouped tab using its live mapping; the restored tab and its
    source stay memory-only unless saved as a case.
21. **Unresolved tokens** are highlighted red; popup offers "Assign to identity…"
    (searchable case identities; then restores) or "Leave as is". Summary
    navigates to the next one. Copy/Save never block; no automatic guessing.
22. **Case-wide edits.** Alias rename, category change, merge/split apply to all
    open member tabs, each as an edit in that tab's own undo history. Undo in
    the originating tab reverts case-wide unless a later case-wide change
    depends on it, in which case undo explains what blocks it. Per-document
    operations (Apply, Keep, mention assignment) stay tab-local.
23. **Retired aliases.** Aliases are never reused; renamed ones stay as retired
    aliases of the same identity. Reopening a member file re-links current and
    retired alias tokens as applied, restorable replacements.
24. **Panel.** Replacements panel adds a collapsed "In case, not in this
    document (n)" section supporting rename, primary wording and add term;
    search covers both.
25. **Leaving / forgetting.** A leaving document keeps its live mapping; the
    case keeps its identities. "Forget identity" (confirmed) removes one
    identity's originals; its aliases become unresolved afterwards.

ADR 0041 will supersede the "never serialized" parts of ADRs 0002/0014/0018.

### Round 5 (confirmed 2026-10-11)

26. **Delivery phases**, each shippable:
    1. Memory-only restore: Paste AI Response from a tab's live mapping,
       lenient matching, unresolved tokens, primary wording, placeholder notice
       (#4–7, #16, #20–21). No storage change.
    2. Saved cases: encrypted store, New case…, autosave, session manifest,
       Open/Delete Case, retired aliases and re-linking (#2, #13–15, #17–19,
       #23, #25). ADR 0041 takes effect.
    3. Multi-document cases: drag/move, joining and renumbering, case-wide
       edits, panel section, add term (#10–11, #22, #24).
    4. Folder drop as a case, then roadmap step 5 conveniences.
27. **Evidence.** Phase 1 gate: CI fixtures of real ChatGPT/Claude replies to
    pseudonymized EN/RU legal text (suffixes, Markdown wrapping, renumbered and
    invented tokens) with expected restorations. Phase 2: case file encrypted,
    tamper/corruption rejected, missing key handled, and no originals in the
    session manifest or app-data grep. Native macOS/Windows sidebar and
    keyboard checks recorded as evidence. No numeric budgets before baselines.

### Amendment during implementation (confirmed 2026-10-11)

Rounds 1–4 assumed a "Copy for AI" action that ADR 0019 had removed. The user
chose to bring **Copy for AI** back as a second copy action carrying the
placeholder notice (#7), and to make restored values **proposals that keep
their aliases** rather than applied replacements (#4, #16): Apply then Copy for
AI sends the next round. Phase 1 places Paste AI Response in the toolbar and the
File menu (mdoc has no Edit menu).

### Phase 2 notes (2026-10-11)

Implemented as recorded in ADR 0041 "Phase 2 as implemented". Narrowings to
revisit: Linux cases disabled until Secret Service support; case names default
to the document name (rename inline); the close question is per tab close, not
quit; closed cases are listed only under File → Open Case….

### Phase 3 notes (2026-10-12)

Implemented as recorded in ADR 0041 "Phase 3 as implemented". Narrowings to
revisit: case-wide undo is propagation rather than a linked, blockable undo
(#22); merges and splits are not pushed to other members; Move to case lists
five recent cases; an added term's category is guessed, not chosen.

### Round 6 — phase 4 (confirmed 2026-10-13)

28. **Folder drop.** Supported files directly in the folder, sorted by name;
    subfolders skipped and counted in the notice.
29. **Folder case.** Dropping a folder creates an empty case named after it at
    once; each document joins when it has Markdown (Markdown at once, PDF/DOCX
    after conversion). The first scan fills the shared mapping.
30. **Clipboard.** In-app command first: "Pseudonymize Clipboard" replaces the
    clipboard text using the active tab's case (or its live mapping), exact
    known names only, no model. A system-wide hotkey is a later, separate step.
31. **Also in phase 4:** a read-only before/after view (original beside
    pseudonymized text, scrolling together) and per-case category switches that
    scans in member documents respect. Empty-page case list stays deferred.

## Frontier

Empty as of round 5. Accepted record: [ADR 0041](../adr/0041-cases-and-ai-response-restoration.md).
