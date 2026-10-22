# Find and replace

Status: Accepted, 2026-10-22.

## Problem

Find could locate text but not change it. Lawyers need Replace and Replace
all, with Match case and Whole word, without breaking Markdown formatting or
the link between an applied alias and its original value.

## Decision

- **Port behaviour, keep the index.** The replace row, option toggles, key
  bindings and Replace all loop follow Zed's GPL-3.0 `search` crate
  (`BufferSearchBar`). mdoc keeps its own source-mapped find index
  (`crates/mdoc-editor/src/search.rs`) and find session. No Zed source is
  copied (the MIT `crates/` must not take GPL code); the app's find session
  credits Zed in its header and THIRD-PARTY-LICENSES.html names the crate.
- **Find searches visible text** (the Markdown editor; the Source preview is
  read-only). A match whose visible characters
  map to more than one source range (it crosses hidden syntax, as "foo bar" in
  `foo **bar**`) is never replaced. Replace moves past it; the status counts
  it as skipped because it crosses formatting.
- **Applied aliases are never touched.** Replace and Replace all skip any
  match that overlaps an applied alias. Replace moves to the
  next match and the status says the alias is changed in the PII panel.
- **Replace all is one undo step.** One transaction covers every replacement.
  If the document changed since matches were computed, Replace all searches
  the current text again first, so it never replaces stale positions.
- **Literal replacement.** The Replace text is inserted as typed: no regex,
  no `$1`, no case preservation. It persists across tabs like the Find text.
- **Status** after Replace all: "Replaced 12 · skipped 2 (formatting) · 1
  (alias)", or just "Replaced 12"; screen readers announce it.
- **Read-only views** (the Source preview, comparisons) show no Replace row;
  the Replace chord opens Find there.
- **Whole word** uses Zed's rule: Unicode letters, digits and `_` are word
  characters, so Cyrillic works; a typographic apostrophe inside a word stays
  part of it.
- **Keys:** ⌘⌥F (macOS) / Ctrl+H (Windows, Linux) open Find with the Replace
  row. Enter in the Replace field replaces and moves next; ⌘Enter /
  Ctrl+Alt+Enter replaces all. ⌥⌘C / Alt+C toggles Match case, ⌥⌘W / Alt+W
  toggles Whole word. Every control is also a button whose tooltip shows its
  chord. The Edit menu has "Find and Replace…".

## Invariants

- Replace never edits hidden Markdown syntax.
- Replace never edits text inside or overlapping an applied alias.
- One ⌘Z / Ctrl+Z reverts a whole Replace all.

## Consequences

- Some matches need a manual fix; the skipped count says how many and why.
- Done when each control works by keyboard and mouse on a long document with
  applied aliases, and one undo reverts Replace all.
