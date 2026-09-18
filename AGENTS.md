# AGENTS.md

Read `/Users/tebriz/.codex/RTK.md` when available; prefix shell commands with `rtk`.

mdoc is a lightweight, file-based Markdown WYSIWYG editor with a side-by-side
PDF viewer. Rust edition 2024 + GPUI; no database or notebook model.

- `src/main.rs`: native window, file actions, unsaved-change prompts, PDF pane.
- `src/document.rs`: UTF-8 loading and atomic saves with external-change detection.
- `src/images.rs`: lazy local image decoding, scoped to the open document.
- `src/style.rs`: WYSIWYG colors.
- `src/ui_tests.rs`: headless GPUI document-flow tests.
- `crates/mdoc-editor`: host-agnostic WYSIWYG editor.
- `crates/gpui-pdf`: virtualized PDF viewer; app enables search and forms.
- `crates/mdoc-markdown`: editor syntax dependency; no reader view in the app.
- `crates/gpui-bidi`: shared bidirectional text layout.
- `crates/os-spellcheck`: standalone editor demo dependency only.

Keep changes surgical. Prefer deleting over adding abstractions or dependencies.
Crates remain host-agnostic and cross-platform. Resolve assets relative to the
Markdown document, never a journal data directory. Do not touch old notebook DBs.
Do not add back journal, graph, whiteboard, or account features.
Preserve Markdown source when rendering; do not rewrite it on load or save.

Before committing, run the full gate:

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Tests belong beside nontrivial logic; chrome-flow tests use GPUI test windows and
throwaway files. Live GPUI synthetic keyboard input is unreliable: shortcuts
need a human check. Never call headless rendering a visual approval.
Use conventional commits with a scope. Keep all three platforms buildable.
