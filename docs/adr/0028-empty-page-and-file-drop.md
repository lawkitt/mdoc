# Empty page and external file drop

Status: Accepted, 2026-10-09. The empty page is superseded by Home in
[ADR 0048](0048-workspace-shell-redesign.md); file drop stands.

## Problem

mdoc accepted no files dragged from the OS.

## Decision

- **External file drop** opens files through the existing open path: anywhere
  on the window while Home or an empty tab shows, and on the document sidebar
  (Chrome-like).
- Sidebar drops show an insertion line and open new tabs at that position; a
  drop never replaces a document. Dragging over the collapsed rail reveals it.
- Drag-over feedback reads "Drop to open", or "No supported files" when nothing
  dragged is supported. Folder drops open as a case (ADR 0041).

## Consequences

All opening flows through `Tabs::open_paths`, which gains an optional insertion index.

References: [interview](https://github.com/lawkitt/mdoc/blob/faa2c5e22a7eb85d4dc598c8f1f21eb125140044/docs/design/empty-page.md).
