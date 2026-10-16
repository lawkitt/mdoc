# Keep editor input and edit history together

Status: Accepted, 2026-10-05. Its deferral of API and feature removal is
superseded by [ADR 0029](0029-codebase-cleanup-charter.md).

## Problem

EditorState interleaved source mutation, undo grouping, clipboard and keyboard
editing, UTF-16 conversion, pointer navigation, painted geometry and menus, so
reviewing an edit's history or an IME conversion meant searching the root file.

## Decision

Source replacements and loads, diagnostic remapping, auto-replacement,
keyboard edit/format/clipboard actions, history snapshots and coalescing, and
the `EntityInputHandler` implementation live in a private `input` module, with
the UTF-16 and grapheme helpers beside them. EditorState remains the sole owner
of source, selection, history, providers and caches; public paths are
unchanged. Pointer navigation and painted geometry stay with rendering. No new
service, trait hierarchy, runtime indirection or separately synchronized state.

## Invariants

Preserve source bytes, caret/selection, notification order, content revisions,
stale-replacement rejection, diagnostic remapping, undo grouping and limits,
redo branching, clipboard routing, auto-replacement and IME conversion.

References: [full text before ADR 0052](https://github.com/lawkitt/mdoc/blob/f368e436cd2d3e70789d51879b9f54f379900e1e/docs/adr/0011-editor-input-module.md).
