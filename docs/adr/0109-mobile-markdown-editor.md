# Mobile Markdown editor

Status: Accepted, 2026-11-09.

## Why

The phone could review a document but not change it: no fixing OCR damage,
no cutting boilerplate, no new document. The desktop editor is GPUI, which
has no phone backend. So the phones edit in a native TextKit 2 view over a
GUI-free Rust editor core that desktop's `mdoc-editor` also calls: each
editing rule (Enter, joins, formatting, lists, tables, paste, undo
coalescing) exists once, over one Markdown parser, and review keeps working
over the edits. The text view's string is the source, saved bytes are the
text as typed, and candidate offsets are the core's UTF-8 offsets. Editing
autosaves with no Save button or version history, and typing joins the
review decisions in one undo history; replies and Restore with mdoc stay
read-only. Plain text leaves only through Copy for AI and Share; Cut and
Copy use a pasteboard only mdoc reads.

## Rejected

- **CodeMirror 6 in a web view**: a second parser, and rules outside the
  core.
- **Rules written in Swift**: they break ADR 0085's rule that the core
  decides.
- **Tags, wikilinks or a vault**: mdoc edits documents, not notes.

## Revisit when

A keystroke drops a frame on the iPhone 12 Pro in `legal-opinion.docx`.
