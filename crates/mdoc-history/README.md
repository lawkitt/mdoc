# mdoc-history

Plain source-edit transactions emitted by [`mdoc-editor`](../mdoc-editor) and
consumed by models that follow its undo history, such as
[`mdoc-pii`](../mdoc-pii).

- `SourceEdit`: one replaced range and the length of its replacement.
- `HistoryChange`: a move between two history states, with sorted edits in
  pre-operation coordinates (empty for undo/redo travel).
- `EditorTransaction`: the changes of one editor operation plus every history
  state still reachable.
- `inverse_edits`: destination ranges of an ordered batch.

`mdoc-editor` re-exports these items at its root, so existing paths remain valid.
The crate has no dependencies, so consumers need no GUI toolkit.
