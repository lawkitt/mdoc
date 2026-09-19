# Markdown text search decisions

Design interview, 2026-09-19. Both rounds and shared understanding are confirmed.
The implementation plan below is requested documentation, not authorization to code.

## Confirmed: round 1

1. **Current document only.** Search the open Markdown document. PDF search
   remains separate; searching other files is outside this release.
2. **Visible Markdown text.** Include code blocks and link labels, exclude
   hidden formatting markers and link destinations. A query `hello world`
   matches `hello **world**`. This follows what the reader sees rather than
   exposing Markdown source syntax as search results.
3. **Literal find only.** Case-insensitive by default, with a Match case
   toggle. Support Unicode text, including Russian. Regex, whole-word matching,
   and replacement are outside this release.

## Current code findings

The editor exposes source search, search highlighting, and offset geometry.
Its source-search helper does not satisfy visible-text semantics: implementation
needs a visible-text projection with original source mapping, including matches
split by inline formatting. PDF already handles pane-local find shortcuts.
Searching must preserve document source, dirty state, selection, and undo history
unless a subsequent confirmed decision explicitly changes selection behavior.

## Confirmed: round 2

4. **Controls and focus.** A compact bar above the Markdown pane, with query,
   Match case, current/total count, previous/next, and close. Cmd/Ctrl-F opens
   or focuses search in the focused pane; Markdown is the default when neither
   document pane is focused. Enter/Shift-Enter in the bar navigates; Cmd/Ctrl-G
   and Shift-Cmd/Ctrl-G navigate from the Markdown pane or its bar. Escape closes
   Markdown search and returns focus to the editor. PDF behavior stays local.
5. **Navigation and lifecycle.** Search as the query changes; start at the first
   match at or after the caret and wrap. Highlight all matches, distinguish and
   scroll to the active one, without moving the caret or selection. Closing
   clears highlights and retains query/case settings for the same document.
   Changing documents closes and resets search. Document edits refresh results
   without scrolling; preserve the active occurrence where possible, otherwise
   choose the next occurrence at or after its former position and wrap. Empty
   query has no highlights; no results displays 0 matches.
6. **Text boundaries and matching.** Match across inline formatting within a
   paragraph, heading, list item, or table cell, but never across block/cell
   boundaries. Treat soft line breaks as spaces, preserve code whitespace,
   and search decoded visible escapes/entities. Exclude image metadata and
   hidden destinations. Use Unicode default case folding without locale-specific
   rules, accent stripping, or Unicode normalization; return non-overlapping
   matches and map folded matches back to original source ranges.

## Implementation plan

Snippets describe proposed interfaces, not existing APIs. Implement in order.

### 1. Visible-text index — `crates/mdoc-editor/src/search.rs`

Keep parsing, source mapping, and matching host-agnostic. Reuse the syntax
recognition in `markdown_syntax.rs` and table parsing in `tables.rs`; do not
introduce a second Markdown dialect or build the index from viewport-only layout.

```rust
pub struct SearchMatch {
    // One occurrence can span several source runs: hello **world**.
    pub source: Vec<Range<usize>>,
}

pub struct SearchIndex {
    segments: Vec<SearchSegment>, // never match across segments
}

struct SearchSegment {
    text: String,
    units: Vec<MappedUnit>,
    // Cache folded text + corresponding source-unit map lazily.
}

struct MappedUnit {
    visible: Range<usize>, // UTF-8 bytes in segment.text
    source: Range<usize>,  // UTF-8 bytes in original Markdown
}

impl SearchIndex {
    pub fn from_markdown(source: &str) -> Self;
    pub fn find(&self, query: &str, match_case: bool) -> Vec<SearchMatch>;
}
```

- Emit one segment per paragraph, heading, list-item paragraph, table cell,
  or code block. Nested blocks remain separate. Hard breaks end prose segments;
  soft breaks emit a mapped space. Code preserves whitespace and newlines.
- Omit syntax, destinations, image metadata, fence delimiters, table separators,
  and generated UI labels. Omitted non-text objects form boundaries rather than
  joining unrelated words. Keep literal text in unsupported constructs consistent
  with the editor. Temporary caret-driven syntax reveal does not change the index.
- Extract/share semantic runs from `markdown_syntax.rs`; its existing
  `hidden_runs` is line- and caret-dependent, so it is not the complete index.
  Decode escapes/entities with source provenance. Verify renderer parity; if
  needed, share decoding with rendering and its source map, never rewrite content.
- Build mappings per Unicode scalar/transformed unit, not one `usize` per byte.
  Merge adjacent source ranges only when genuinely contiguous; keep syntax gaps.

```text
source:  hello **world**
visible: hello world
match:   source = [0..6, 8..13]     // one result, two highlight ranges

source:  A &amp; B
visible: A & B
query &: source = [2..7]           // decoded glyph maps to whole entity
```

### 2. Literal Unicode matcher — same module

```rust
// Conceptual pipeline:
let needle = if match_case { query.to_owned() } else { case_fold(query) };
for segment in &index.segments {
    let haystack = segment.mapped_text(match_case);
    // Find non-overlapping occurrences in this segment only.
    // Map every covered unit to its complete original source range.
    // Coalesce contiguous ranges; deduplicate identical source occurrences.
}
```

Use Unicode default full case folding: `Straße` matches `STRASSE`; final sigma
matches sigma. `to_lowercase()` is insufficient. First check existing dependencies
for a suitable implementation; otherwise use one small, maintained, cross-platform
dependency, verifying its API/version at implementation time. Do not hand-maintain
Unicode tables. Case-sensitive mode compares exact decoded text. Neither mode
strips accents or normalizes Unicode. Empty query returns zero results.

Retain `find_in_source` with its existing semantics for existing callers; the
app must use `SearchIndex`. Cache projection/folding across query changes and
invalidate on content changes, not on selection, scrolling, or theme changes.

### 3. Grouped highlights and exact geometry — editor `lib.rs`, `element.rs`

```rust
pub fn set_search_matches(
    &mut self,
    matches: Vec<SearchMatch>,
    active: Option<usize>,
    cx: &mut Context<Self>,
);

// Current layout; first visible glyph of the occurrence, including wrap row.
pub fn search_match_bounds(&self, index: usize) -> Option<Bounds<Pixels>>;
```

Keep `set_search(Vec<Range<usize>>, ...)` as a thin compatibility adapter. Store
logical occurrences; in `element.rs`, run existing `range_quads` over each source
range and apply the same active color to every range of that occurrence.

`offset_screen_top` currently returns a logical line's top; extend geometry for
wrapped lines/table cells. For offscreen matches, scroll approximately to the
logical row, then refine after layout to the actual glyph. Tag deferred scrolling
with document/query revision and active occurrence so stale work cannot jump.
Never call `set_cursor` to reveal a result. Recompute geometry after wrapping,
resizing, or focus-driven syntax reveal; avoid repeated scroll loops.

### 4. Find bar and state — `src/markdown_search.rs`, `src/main.rs`

```rust
struct MarkdownSearch {
    open: bool,
    input: Entity<SearchInput>,
    match_case: bool,
    index: SearchIndex,
    matches: Vec<SearchMatch>,
    active: Option<usize>,
    revision: u64,
}

enum RefreshReason { Query, Content }

// Workspace owns state, editor owns highlights, input owns text composition.
fn open_find(/* ... */);
fn refresh_find(reason: RefreshReason /* ... */);
fn step_find(backward: bool /* ... */);
fn close_find(/* ... */);
fn reset_find_for_document(/* ... */);
```

Use a small single-line GPUI `SearchInput` implementing `EntityInputHandler`,
with selection, clipboard, UTF-16 conversion, and IME composition. Inspect GPUI's
available input examples before implementing; do not use the Markdown editor as
the input field or copy PDF's rudimentary key-to-string query editing.

```text
Markdown column (flex column, min-height: 0)
├─ find bar: [query] [Match case] [3/12] [Previous] [Next] [Close]
└─ existing document-scroll (flex: 1, min-height: 0)
```

Keep the bar outside `document-scroll`; preserve the PDF column. Opening focuses
the query; reopening selects its retained text. Empty query shows `0/0`; nonempty
query without results shows `0 matches`. Disable navigation without matches.
Give controls stable element IDs, focus feedback, and accessible labels.

### 5. Actions, refresh, and lifecycle — `src/main.rs`

| Trigger | State transition / effect |
| --- | --- |
| Find in Markdown/bar | Open or focus query; initialize active at/after caret, wrapping |
| Find with PDF focus | Existing PDF handler consumes it; no Markdown search change |
| Find with neither pane focused | Open Markdown search |
| Query / Match case change | Find against cached index; choose at/after caret; scroll |
| Next / previous | Wrap active index; scroll; preserve editor selection |
| `EditorEvent::Changed` | Invalidate projection; refresh if open; never auto-scroll |
| Escape / close button | Clear highlights; retain query/case; focus editor |
| Successful New / Open / Import | Reset bar/query/case/results and invalidate pending work |
| Failed/cancelled document transition | Preserve search state |
| Save / Save As / PDF replacement | Preserve Markdown search state |

Bind Enter/Shift-Enter/Escape only in the search-input context. Bind find-next
shortcuts in Markdown editor/bar contexts; leave normal editor Enter untouched.
Ensure the PDF handler stops propagation before any host fallback.

For content refresh, map the old active source anchor through the edit. If no edit
delta is exposed, derive a UTF-8-safe common-prefix/suffix replacement from old/new
text. Preserve a surviving occurrence; otherwise pick the first match at/after
the mapped position and wrap. While closed, mark the index stale and rebuild on
reopen. Reset only after an accepted document replacement in `Workspace::proceed`,
not when opening a dialog or starting conversion.

Cache the index; start with synchronous pure matching and measure large-document
latency. If work blocks interaction, move projection/matching to GPUI background
tasks with document/query revision checks; publish only current results and never
show stale highlights. Avoid an arbitrary result cap that misreports totals.

### 6. Verification and delivery

| Layer | Required cases |
| --- | --- |
| Projection | Inline formatting split; links exclude URLs; images excluded; headings/lists/quotes; table-cell isolation; soft vs hard breaks; code whitespace; escaped punctuation/entities; malformed syntax |
| Matching | Cyrillic case; `ß` expansion; sigma; emoji/multibyte offsets; accent distinction; composed/decomposed distinction; case toggle; empty/no match; non-overlap; folded-source deduplication |
| Editor | Grouped active highlights; wrapped-line/table geometry; offscreen reveal; last result; all mapped offsets are valid UTF-8 boundaries |
| Host (`src/ui_tests.rs`) | Counts/wrap; open/close/refocus; content refresh without scrolling; successful vs cancelled transitions; Save As; PDF isolation; stale deferred work |
| Invariants | Search/query/navigation leave text, dirty state, selection, caret, and undo history unchanged |
| Performance | Measure index build, repeated query, and edit-refresh latency on small and multi-megabyte Markdown; verify typing/scrolling remains responsive |
| Human acceptance | macOS/Windows/Linux shortcuts and IME; focus routing with PDF open; light/dark highlights; resize/wrapping; no layout jump on close |

Place pure tests beside `search.rs`; exercise rendering/flows with GPUI test
windows and throwaway files. Headless geometry tests do not approve visuals.
Record unavailable platform checks explicitly.

```sh
rtk cargo fmt --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
rtk git diff --check
```

Suggested implementation checkpoints:

```text
1. feat(editor): index visible markdown text for literal search
2. feat(editor): highlight grouped search matches and expose match geometry
3. feat(app): add pane-local markdown find bar and lifecycle integration
```

Run the full gate before committing. Complete the matrix and report human/platform
checks separately from automated results; no implementation changes are part of
this planning task.
