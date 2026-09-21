# Sidebar tabs

Status: implemented; automated validation recorded below. Native visual/shortcut checks remain manual.

## Confirmed

- One tab owns one Markdown document, saved or untitled, and at most one attached
  PDF/DOCX. Switching selects the whole pair.
- Each live tab retains text, undo history, selection, scroll, search, and preview
  state. Switching tabs never prompts to save.
- Closing the preview hides it without forgetting its attachment. The user can
  reopen it, including before saving the Markdown. Attachment identity and pane
  visibility must be separate.
- Collapsible left sidebar lists open tabs only, with close buttons, dirty
  indicators, drag reordering, and a plus icon for a new empty Markdown tab.
  No filesystem tree, recent-files list, or horizontal tab strip.
- Opening Markdown creates a tab; reopening the same file activates its existing
  tab. Opening PDF/DOCX attaches it to the active tab. Import as Markdown creates
  a new paired tab.
- Restore tabs from the previous session. Keep persistence simple and performant;
  restore saved paths only, not unsaved buffer contents. Quitting still requires
  Save/Discard/Cancel for dirty or untitled documents. No draft recovery in v1.
- Across restart preserve tab order, active tab, attachments, sidebar visibility,
  preview visibility, editor caret/scroll, and preview page/zoom. Undo and search
  survive live switching but reset after restart. Hidden previews load on demand.
- Closing the last tab leaves one empty Markdown tab. Closing a dirty tab prompts
  Save/Discard/Cancel.
- Attaching another PDF/DOCX replaces the old attachment only after the new preview
  loads successfully. Closing the preview hides it; its attachment remains.
- Imports and DOCX conversion continue across tab switches, with completion bound
  to the originating tab. Closing that tab cancels its work. One import at a time
  globally; DOCX conversions are serialized.
- Busy tabs show a spinner or other loading indicator, including inactive tabs.
  Queued work must be distinguishable from running work. Errors remain visible on
  the affected tab and never replace another tab's content.
- Retain initialized editors and previews until their tab closes, including hidden
  previews. No eviction system in v1; memory grows with opened/initialized content.
- Missing Markdown remains as an unavailable tab with Retry/Close controls; never
  silently replace it with an empty document. Missing attachments leave Markdown
  usable. Restore current disk contents and clamp saved reading positions.
- On quit, resolve all dirty tabs before closing any. Cancel keeps tabs open
  (completed saves remain saved). Checkpoint the final session before teardown,
  without recording teardown as user-initiated tab closures. A corrupt session
  file shows a notice and opens an empty tab.
- Cmd/Ctrl+N creates a tab; Cmd/Ctrl+W closes it; Ctrl+Tab and Ctrl+Shift+Tab cycle
  tabs. A sidebar button toggles visibility. Duplicate filenames show parent paths.
- Save As targeting a file already open in another tab is blocked, with an offer
  to switch to that tab. File identity checks also drive open-file deduplication.

## Reference inspection

Zorite upstream revision: `3f14fafba120386b058e3cde24ced55f23dd653e`.

| Source | Reuse |
| --- | --- |
| `src/ui/sidebar.rs` | Collapsible rail, row styling, scrolling/layout patterns |
| `src/ui/tab_bar.rs` | Activation, close controls, truncated labels, reorder interactions |
| `src/app.rs` | Keyboard cycling, retained PDF views, explicit PDF resource release |

Adapt interaction patterns to vertical tabs. Do not import notebook/database
ownership or page-editor reconstruction on activation: unsaved buffers and undo
history must survive switching. Current mdoc `Workspace` owns a single editor,
document session, preview, and search; these need per-tab ownership. Its current
preview close clears the source, so hiding and detaching need distinct semantics.

## Persistence

- One versioned JSON session file in the platform's local application-data folder.
- Atomic replacement through a temporary file; serialize writes and coalesce
  metadata changes. Existing `serde_json`, `dirs`, and `tempfile` dependencies
  suffice; no database or new framework.
- Ordered tab records, active tab, Markdown path, attachment path, preview
  visibility, sidebar visibility, and agreed reading positions. Store original
  DOCX path, never temporary PDF.
- Restore sidebar records first; load editor/preview resources on first activation.
- Save/Discard/Cancel remains necessary on quitting. A path list does not preserve
  unsaved edits; untitled tabs discarded on quit cannot restore their attachments.

## Performance and correctness acceptance

- Switching initialized tabs performs no disk reads, parsing, conversion, editor
  reconstruction, or session-file writes synchronously on the UI thread.
- Only the active tab's document/preview is rendered. Loading indicators must not
  cause hidden document layouts or repaint the entire workspace unnecessarily.
- Restore lightweight sidebar records first. Lazy initialization must not eagerly
  parse every Markdown file, decode every image, or convert every attached DOCX.
- Coalesce session changes and perform serialized atomic writes off the UI thread;
  no per-keystroke persistence. Persist metadata only, never Markdown content,
  undo stacks, rendered pages, or temporary DOCX conversion artifacts.
- Async callbacks use stable tab identity plus operation generation; stale results
  cannot overwrite a newer attachment, affect another tab, or resurrect closed tabs.
- Closing tabs releases editor/image resources and PDF CPU/GPU resources, cancels
  queued/running work, and disposes DOCX temporary files in the correct order.
- Verify retained source, dirty state, undo, selection, reading positions, and
  attachment identity across tab switches and preview hide/reopen.
- Measure startup and switching with multiple tabs, including long Markdown and
  PDF/DOCX pairs. Check repeated open/close cycles for resource growth. Report
  measured results and remaining limitations; do not infer visual approval from
  headless tests. Native keyboard behavior requires a human check.
- Before committing, run the repository's full fmt, clippy, and workspace-test gate.

## Implementation and validation

- `src/tabs.rs`: sidebar, stable tab IDs, retained document entities, lazy loading,
  conversion queue, close/quit coordination, and coalesced session checkpoints.
- `src/session_store.rs`: versioned paths-only JSON with atomic replacement.
- Each initialized tab keeps its existing `Workspace` editor/search/preview view.
  OCR setup and import admission are shared; hidden tabs are not rendered.
- No added dependencies, database, eviction policy, or draft-recovery framework.
- Session snapshots are sampled every two seconds and written only when metadata
  differs; close/quit also checkpoint. Writes are serialized with one latest
  pending snapshot. Normal quit waits for the final checkpoint before teardown.
- Import cancellation is cooperative at conversion-stage boundaries because the
  underlying conversion calls are synchronous. Closing a tab immediately rejects
  its result; the global import slot stays occupied until the call returns.
  DOCX worker cancellation also terminates/reaps its child process.
- Saving a previously untitled pair retains its attachment. An empty untitled tab
  with an attached preview prompts on close/quit, so its pairing is not silently
  discarded. Empty unattached tabs close without prompting.

Measured on this macOS host, debug profile, headless GPUI, 1280×800 viewport;
24 session records, six initialized tabs, including two 127.5 kB Markdown files
and PDF/DOCX pairs. These are CPU measurements, not GPU presentation latency.

| Measurement | Median | p95 |
| --- | ---: | ---: |
| Retained tab activation handler | 0.007 ms | 0.018 ms |
| Metadata snapshot, 24 tabs | 0.007 ms | 0.008 ms |
| Short Markdown + preview redraw | 12.060 ms | 12.492 ms |
| Long Markdown redraw after switching | 292.034 ms | 301.045 ms |
| Same long Markdown redraw without switching | 295.649 ms | 321.866 ms |

Cold restoration with a long Markdown tab active took 1,062 ms including its
initial headless rendering; only one of 24 document views was initialized.
Long-document rendering remains a substantial debug-profile cost even without
tab switching; these results do not establish 60 Hz rendering for large files.

Five further DOCX open/close cycles released every viewer entity and temporary
PDF. RSS went from 386,112 KiB before those cycles to 378,128 / 378,208 / 378,224 /
378,224 / 378,224 KiB. RSS is observational; explicit entity/file lifetime checks
are the cleanup assertions.

Reproduce measurements:

```sh
cargo test -p mdoc tabs_host_performance -- --ignored --nocapture --test-threads=1
```

Final gate: `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace` passed (304 passed, 8 ignored). The ignored tab
performance test was also run explicitly and passed.

Automated cases cover tab switching/undo/search, ordering, close/cancel, last-tab
replacement, missing-file retry, duplicate-path save protection, lazy restoration,
hidden preview reopening, attachment replacement failure, background import
routing/focus, queued conversions, final checkpoints, and DOCX resource release.
Native drag/keyboard interaction, GPU presentation, and Windows/Linux execution
still require platform checks; headless coverage is not visual acceptance.


## PDF resizing and sidebar polish

- PDF and DOCX previews both start in sticky fit-to-width mode. Sidebar and window
  resizing use the viewer's existing fit logic; explicit manual zoom opts out.
- Session records retain Width/Page/Manual sizing policy as well as numeric zoom.
  Older manifests without a policy use Width, so their previews resize too.
- Regression: opening the sidebar previously left a fresh PDF at zoom 1.0 in both
  pane widths. The test now verifies zoom decreases on opening and returns on
  collapse; restoration and manual-zoom preservation are also covered.
- Sidebar styling uses the editor's dark/light palette and green accent, inset
  rounded rows, 13 px document labels, compact attachment badges, and consistent
  28 px header / 24 px close controls. Loading labels remain static, with no new
  animation or background work.
