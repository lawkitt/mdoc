# Rendered Markdown on mobile

Status: Accepted, 2026-11-03. Amends [ADR 0085](0085-mobile-apps.md)'s
Text display rule and [ADR 0087](0087-ios-ui-polish.md)'s review-text
styling. The separate table spike was skipped (the user's call): the
implementation itself must meet the pass test below.

## Problem

The iOS app shows a document's Markdown source with dimmed markers (ADR
0085), so candidate offsets are the core's source offsets. On a phone that
reads poorly: tables are rows of pipes in a small monospaced font, bold
keeps its `**`, lists have no bullets or indent, and escapes show their
backslashes. Desktop hides the markers and draws tables, lists, quotes,
alerts and code as blocks. The phone should read as well as desktop.

## Decision

- **The text is rendered; the source stays underneath.** Every Markdown
  marker is hidden, in a selection too, as on desktop; `\_` reads `_`. The
  view's text is still the source, so candidate offsets, Replace… and Copy
  for AI do not change. The text stays read-only.
- **One TextKit 2 view with drawn decorations** (option A). Hidden markers
  take no width; table source lines take no height; a custom layout
  fragment draws bullets and numbers, quote bars, alert and code cards and
  rules; a table is an embedded view in place of its lines. On a
  bilingual three-column contract the build must show, in that one view: wrapped
  cells with sideways scrolling, table lines at no height, a tap on a name
  in a cell opening its sheet, and Replace… inside a cell hitting the right
  source range. If one of these fails, the text becomes a stack of block
  views, each with its source offset (option B). VoiceOver table reading
  and smooth scrolling on an iPhone 12 Pro (50 pages, 20 tables) are
  required too, but a miss there is fixed inside A. A web view is not
  used.
- **Tables.** Cell text wraps inside columns at least about 140 pt wide,
  scaled with Dynamic Type, and never narrower than the column's longest
  word, so no word breaks inside; a document's stored column widths
  (`<!-- table:… -->`) set the proportions and its table style is drawn.
  A bold header row with a light fill, thin grid lines, and the table's own
  sideways scroll with an edge fade. Candidates in cells look and act as
  in text. A selection stays inside one cell or outside tables. VoiceOver
  reads a table as a data table (row, column, column header). No full-screen
  table view for now.
- **Lists** as desktop draws them: `•` at every depth, ordered items
  `1.` → `a.` → `i.` by depth and renumbered (`ordered_marker`), a hanging
  indent, task items as ☐ / ☑ that do not toggle.
- **Other blocks.** Headings at desktop's relative sizes and spacing; quote
  bars; alert cards with their title; code cards with the fences hidden;
  drawn rules; footnote definitions and raw HTML greyed. Math shows as
  monospaced source and an image as an "Image" chip until a later decision.
- **Type.** iOS text styles, so Dynamic Type applies, scaled to desktop's
  heading sizes and paragraph and list spacing. ADR 0087's rules stay: no
  hyphenation, short blank lines, stronger dark-mode fills.
- **Show Markdown.** A ⋯ menu beside Identities on the document screen
  holds a Show Markdown checkmark that switches to the source view of ADR
  0085. Off by default; one setting for the app. Phone editing, if it
  comes, is expected to start in this view.
- **Everywhere text is shown.** The document's Text pane, the Reply pane
  and the Restore with mdoc share extension all render, and all follow Show
  Markdown.
- **Recognition comes from the core.** What desktop hides and decorates
  (hidden runs, list prefixes and depth, task state, alert kind, table
  cells, widths and style, writing direction) moves from `mdoc-editor`'s
  `markdown_syntax` into `mdoc-markdown`; desktop calls it from there and
  `mdoc-ffi`'s `markdown_styling` carries it to the apps. Android renders
  from the same data later.

## Invariants

- ADR 0085's rules hold: read-only text, candidate offsets are the core's
  source byte offsets, Copy for AI bytes identical to desktop, every string
  in the String Catalog, VoiceOver and Dynamic Type on everything new, no
  network but model downloads.
- Moving the rules into `mdoc-markdown` does not change desktop's
  rendering; tests pin desktop's hidden runs before and after the move.
- Test documents copy the structure of real ones, never their names or
  numbers.

## Consequences

- Delivery: a PR stack, no separate spike: (1) rules into
  `mdoc-markdown` with desktop on them, (2) hidden markers, lists, headings,
  quotes, alerts, code, rules and Show Markdown, (3) tables. Each PR gets a
  Codex review before it opens.
- Synthetic survey documents mirror a bilingual RU/EN/ZH contract table, a
  Russian contract with bold and italic terms, a numbered loan agreement,
  an itinerary and a prompt-guide table; iOS and desktop shots of each in
  light, dark and the largest text size go in `docs/evidence/ios-app`.
  `docs/platform-acceptance.md` section 13 gains a rendering row.
- Android renders from the same core data later (#569).
- Found alongside, filed separately: an itinerary PDF imported as one
  paragraph (#565), letter-spaced text reversed on import (#566), a pipe
  table whose source rows are merged (#567), booking codes not proposed
  (#568).
- Amended 2026-11-03 while building it: bullets are `•` at every depth,
  as desktop paints them (the grill had assumed `•` `◦` `▪`); a column is
  never narrower than its longest word; `***bold italic***` is one
  construct in `mdoc-markdown`, which fixes a stray `*` on desktop too.
- Interview and findings: [iOS Markdown rendering grill](../design/ios-markdown-rendering-grill-2026-11-03.md).
