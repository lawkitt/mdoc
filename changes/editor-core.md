- **The editor's rules live in one GUI-free crate.** Enter and ⇧Enter,
  joins on ⌫/⌦, formatting toggles, indent, Turn into, task boxes, table
  commands, paste clean-up, the undo history, search and the reveal rule
  move from `mdoc-editor` into `crates/mdoc-editor-core`, which the mobile
  apps will call too ([ADR 0109](docs/adr/0109-mobile-markdown-editor.md)).
  The desktop editor behaves as before.
