# Mobile Markdown editor

Status: Accepted, 2026-11-09; amended 2026-11-25 ([#1209](https://github.com/lawkitt/mdoc/issues/1209)).

## Why

The phone could review a document but not change it: no fixing OCR damage,
no cutting boilerplate, no new document. The desktop editor is GPUI, which
has no phone backend. So iOS edits in a native TextKit 2 view, and Android
in its own later design, over a GUI-free Rust editor core that desktop's
`mdoc-editor` also calls: each
editing rule (Enter, joins, formatting, lists, tables, paste, undo
coalescing) exists once, over one Markdown parser, and review keeps working
over the edits. The text view's string is the source, saved bytes are the
text as typed, and candidate offsets are the core's UTF-8 offsets. Editing
autosaves with no Save button or version history, and typing joins the
review decisions in one undo history; replies and Restore with mdoc stay
read-only. Copy for AI and Share send only a whole Ready document. A
selection copied by hand is the user's own: never blocked, labelled for
what it holds, marked sensitive unless clean.

## Rejected

- **CodeMirror 6 in a web view**: a second parser, and rules outside the
  core.
- **Rules written in Swift**: they break ADR 0085's rule that the core
  decides.
- **Tags, wikilinks or a vault**: mdoc edits documents, not notes.
- **No plain Copy on a phone** (the first version): it protected nothing
  once a document was Ready, and stopped a user taking one clause.

## Revisit when

A keystroke drops a frame on the iPhone 12 Pro in `legal-opinion.docx`.
