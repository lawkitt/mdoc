# Mobile Markdown editor

Status: Accepted, 2026-11-09.

## Problem

The phone could review a document but not change it: no fixing OCR damage,
no cutting boilerplate, no new document. The desktop editor (`mdoc-editor`)
is GPUI, which has no iOS or Android backend.

## Decision

- **Edit any document, as desktop does:** imported, scanned, pasted and new
  blank ones. Review keeps working over the edits: proposals and decisions
  follow them, as `mdoc-pii` does for desktop.
- **Native TextKit 2 plus a shared Rust editor core.** The rendered text
  view becomes editable. Editing rules (Enter, joins, formatting, lists,
  tables, paste, undo coalescing) move out of `mdoc-editor` into a GUI-free
  crate that desktop and both phones call. Plain typing reaches the core as
  coalesced edits; structural keys call it.
- **Reveal rule:** a core option, so the phone and desktop can differ.
- **Edit mode** is a mode of the review screen (Edit, Done); a new document
  opens in it. It works with Show Markdown on or off.
- **Saving:** autosave after a typing pause, on leaving the document and on
  going to the background; no Save button, no version history.
- **One undo line:** typing joins the core's undo history as coalesced
  "Typing" steps beside review decisions, 256 steps per open session.
- **Edits over names** behave as on desktop: an edit overlapping an applied
  alias stops tracking that mention; an edit inside a pending candidate
  drops it. Discovery runs after a typing pause; recognition only on demand.
- **Tables** are edited cell by cell; typing changes only that cell's bytes.
- **Clipboard:** Cut and Copy in edit mode use a pasteboard only mdoc reads;
  plain text leaves only through Copy for AI and Share.
- **Acceptance:** no dropped frame per keystroke on the iPhone 12 Pro in
  `legal-opinion.docx`; a typing fuzz test keeps saved bytes equal to the
  view's string.

Rejected: CodeMirror 6 in a web view (a second parser, rules outside the
core, web accessibility) and rules written in Swift (breaks
[ADR 0104](0104-mobile-app-structure.md), doubles Android).

## Invariants

- The text view's string is the source; saved bytes are the text as typed
  (ADR 0030, ADR 0060).
- Candidate offsets are the core's UTF-8 source offsets.
- One Markdown parser (`mdoc-markdown`); each editing rule exists once, in
  the core.
- Replies and Restore with mdoc stay read-only.

## Consequences

- Desktop's `mdoc-editor` calls the extracted core, pinned by its tests.

References: [mobile editor grill](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/design/mobile-editor-grill-2026-11-09.md); rules in [docs/specs/mobile.md](../specs/mobile.md).
