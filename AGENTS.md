# AGENTS.md

Read `/Users/tebriz/.codex/RTK.md` when available; prefix shell commands with `rtk`.

mdoc is a local document-preparation utility for lawyers, producing editable
Markdown for AI agents in other tools. It has PDF/DOCX previews, Markdown find,
and optional local OCR. Rust edition 2024 + GPUI; no database or notebook model.
Read `CONTEXT.md` for domain terms, `ROADMAP.md` for accepted priorities and
deferred work, and `docs/adr/` for durable behavior/architecture decisions.

- `src/main.rs`: native window, file actions, unsaved-change prompts, preview pane, OCR status, Markdown find bar.
- `src/document.rs`: UTF-8 loading and atomic saves with external-change detection.
- `src/document_session.rs`: accepted document identity and import provenance.
- `src/tabs.rs` + `src/session_store.rs`: lazy multi-file opening, retained tab views, paths/view metadata restoration.
- `src/import_session.rs` + `src/preview.rs` + `src/search_session.rs`: pending jobs, preview ownership, revision-aware search scheduling.
- `src/docx_preview.rs`: local DOCX-to-PDF conversion worker (size/time caps; rejects macros, tracked changes, encrypted files).
- `src/docx_comments.rs` + `src/comment_panel.rs`: DOCX comment extraction + read-only side list; never enters editor text.
- `src/import.rs`: conversion boundary (AnyDoc for office formats, pdf-inspector for PDFs); performs no writes.
- `src/ocr.rs`: explicit offline OCR setup and runtime paths (Apple Silicon macOS and Windows x64 only).
- `src/markdown_search.rs`: Markdown find UI wiring over `mdoc-editor` search.
- `src/images.rs`: lazy local image decoding, scoped to the open document.
- `src/style.rs`: WYSIWYG colors.
- `src/ui_tests.rs`: headless GPUI document-flow tests.
- `crates/mdoc-editor`: host-agnostic WYSIWYG editor plus rendered-text search.
- `crates/gpui-pdf`: virtualized PDF viewer; app enables search and forms.
- `crates/mdoc-markdown`: editor syntax dependency; no reader view in the app.
- `crates/gpui-bidi`: shared bidirectional text layout.
- `crates/os-spellcheck`: standalone editor demo dependency only.

Keep changes surgical. Prefer deleting over adding abstractions or dependencies.
Crates remain host-agnostic and cross-platform. Resolve assets relative to the
Markdown document, never a journal data directory. Do not touch old notebook DBs.
Do not add back journal, graph, whiteboard, or account features.
Preserve Markdown source when rendering; do not rewrite it on load or save.
Keep extraction/OCR algorithms in the AnyDoc/pdf-inspector forks. Planned
pseudonymization uses Rust-only integration and existing libraries/models;
keep detection/review policy in the app and editor extensions generic.
Preserve compilation speed and warm rebuilds. Cache cleanup is manual:
`rtk cargo clean --workspace` retains dependency artifacts; `rtk cargo clean`
resets the whole target directory. Do not add automatic eviction or profile
changes solely to meet an arbitrary cache-size limit.

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
