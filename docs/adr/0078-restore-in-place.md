# Restore in place

Status: Accepted, 2026-10-29. Amends
[ADR 0041](0041-cases-and-ai-response-restoration.md) (Restore) and
[ADR 0046](0046-workspace-polish-from-anoni-review.md) (view switcher).

## Problem

Restoring an AI reply opens a new tab (ADR 0041). For the common case, reading
the answer with real names, that is a detour: a tab to find, then close, and
the restored values arrive as proposals to review. Anoni 0.86.1 restores in a
reply box beside the export controls, live, in the same document.

## Decision

- **Reply box in Share.** While aliases are applied or a reply exists, the
  Share panel has an "AI reply" box under Copy for AI. Pasting or typing restores at once, with
  ADR 0041's matching rules (lenient separators, Markdown wrappers kept,
  partial matches never restored, no automatic RU inflection).
- **Reply view.** The view switcher gains a fifth segment, **Reply**
  (Edit · Original · Pseudonymized · Side by side · Reply), shown while a
  reply exists; filling the box switches to it: any change that leaves a
  reply while another view shows, a paste over an earlier reply included. It is read-only and follows
  mapping changes live, as Side by side does (ADR 0042). Unknown alias-shaped
  tokens are highlighted as unresolved; nothing is guessed.
- **Summary line** under the box: "N values restored · M unrecognized", with
  **Next unrecognized** when M > 0. N counts distinct identities restored and
  M distinct unknown tokens, as Anoni does; Next unrecognized still visits
  every occurrence.
- **Actions:** **Copy reply** (the restored text) and **Open as tab**, which
  makes today's restore tab: restored values as proposals keeping their
  aliases. No save-to-file action; Open as tab, then Save, covers it. When
  nothing can be restored (a reply kept after its aliases were removed), both
  are off and a line says why; Open as tab has Paste AI Reply's
  no-identities guard and message.
- **Paste AI Reply stays.** The File menu item, ⌥⇧⌘V and case actions keep
  opening a restore tab as in ADR 0041. The box replaces Share's old
  "Paste AI reply" button.
- **Plain text box.** The box keeps exactly what is pasted or typed: no
  Markdown paste or Enter rules (math fences, table cells, paragraph breaks),
  one newline per Enter. A long reply scrolls inside the box with the caret
  kept in view.
- **Kept with the document.** One reply per document. In a case it is stored in
  the case file, encrypted like the mapping; without a case it lives only while
  the tab is open. An unsaved (untitled) tab in a case keeps its reply in
  memory until its first save, which stores it. Clearing the box removes it,
  from the case too.
- **Aliases followed.** The reply is kept in alias form, never as restored
  text. When the document's aliases change under it (joining a case
  renumbers them, a case-wide rename arrives, or Undo / Redo of either), the
  reply's alias tokens are rewritten to the aliases their identities hold
  now, so it keeps restoring to the same people; the case stores the
  rewritten reply. A reshaped generated token (`person 1`) keeps its shape.
  Restoring prefers an identity's current alias over another's retired one.

## Invariants

- The reply's restored text never reaches the session manifest or plaintext
  app data.
- Captured replies (`tests/fixtures/restore/`, ADR 0056) restore completely in
  the Reply view as in the restore tab.

## Consequences

Two ways to restore: in place for reading, a tab for working the reply into a
new document. A case file grows by one reply per member.

References: [interview](../design/roadmap-grill-2026-10-29.md).
