# Empty page and external file drop

Status: Accepted and implemented, 2026-10-09; native file-drag and Windows/Linux checks outstanding. See the [design interview](../design/empty-page.md).

## Problem

A blank Markdown tab shows only a one-line placeholder at top-left. It does not
make opening files obvious, hide that several files can be opened at once, and
mdoc accepts no files dragged from the OS.

## Decision

- An **empty page** — centered icon, **Open files…** primary button, supported
  formats and multi-select hint, ⌘O hint, "or just start typing" — replaces the
  placeholder whenever a Markdown tab's content is empty. It is a non-blocking
  overlay: the editor keeps focus.
- **External file drop** opens files via the existing open path on the whole
  window while the empty page shows, and on the document sidebar (Chrome-like).
- Sidebar drops show an insertion line and open new tabs at that position;
  a drop never replaces a document. Dragging over the collapsed rail reveals
  it. The empty page shows "Drop to open", or "No supported files" when nothing
  dragged is supported.
- Recent files and folder drops are out of scope; folders are skipped with the
  existing notice. Tab groups (Chrome-style) are a roadmap item.

## Consequences

Adds an external-path drop target and drag-over feedback; all opening still
flows through `Tabs::open_paths`, which gains an optional insertion index.
