# mdoc-editor

mdoc's Markdown editor, built on GPUI's text primitives: an
`EntityInputHandler` for keyboard and IME input, per-line shaping, and a custom
`Element` that lays out and paints lines, caret and selection. Internal to the
mdoc workspace (`publish = false`).

It owns the Markdown source, selection, undo/redo history and edit
transactions. With a `SyntaxStyle` installed it renders live-preview
(WYSIWYG) Markdown — headings, emphasis, links, lists and tasks, tables,
GitHub alerts, code blocks, images and highlights — revealing syntax near the
caret; without one it is a raw-text editor. Saved bytes are always the exact
source. Standard Markdown only ([ADR 0030](../../docs/adr/0030-render-standard-markdown-only.md)).

The host supplies images, file chips, search matches and source annotations
(pseudonymization), and receives `EditorEvent`s. Right-to-left lines are laid
out through `gpui-bidi`. Edit transactions come from `mdoc-history`.

Spell-check hooks (`Diagnostic`, `set_diagnostics`, `on_suggest`) are kept as
groundwork for a deferred experiment; `cargo run -p mdoc-editor --example demo`
wires them to `os-spellcheck`.

MIT-licensed (see [LICENSE](LICENSE)), derived from Zorite's editor.
