# iOS Markdown rendering grill, 2026-11-03

Interview record for making the iOS document text read like desktop's
rendered editor: tables, lists, hidden markers and the other blocks. The
decisions go into [ADR 0096](../adr/0096-rendered-markdown-on-mobile.md)
once the tree is closed; this file keeps the findings, the answers and the
open branches.

## Starting point

Phone screenshots (2026-11-03, iPhone 12 Pro) of four documents: a prompt
guide with a pipe table, a Russian supply contract, an air itinerary, and
desktop shots of a bilingual RU/EN/ZH contract table and a numbered loan
agreement.

- **iOS shows the Markdown source** with markers dimmed, by ADR 0085's Text
  display rule, so candidate offsets are the core's source byte offsets.
  Pipes, `**`, `*`, `- ` and `\_` stay on screen; tables are monospaced
  source rows (ADR 0087).
- **The core gives iOS** inline spans and six line roles (heading, quote,
  code block, table header, table row, rule) through `markdown_styling`.
  Nothing about lists, list depth, task items, table cells or alerts.
- **Desktop** (`mdoc-editor`, GPUI) hides markers everywhere except the
  construct under the caret; draws tables as grids with their own
  horizontal scroll, stored column widths and table styles; paints bullets
  and `1.` → `a.` → `i.` numbers (`ordered_marker`) with a hanging indent;
  draws quote bars, alert cards, code cards and rules; greys footnote
  definitions and raw HTML.

## Researched editors (user's research, checked 2026-11-03)

Read as technique references only. The app takes no third-party Swift
packages, and each of these parses Markdown itself, where mdoc's
recognition comes from the core.

- **swift-markdown-view** (MIT): hides markers with a clear colour, a
  near-zero font and a negative kern; a custom `NSTextLayoutFragment` draws
  quote bars, rules, code backgrounds and table row lines. Tables are
  tab-stop columns, so a long cell cannot wrap inside its column.
- **Netherite** (GPL-3.0): hides markers with a 0.01 pt clear font and
  collapses hidden lines with a 0.01 pt line height; tables, callouts and
  math are snapshotted to images by a hidden `WKWebView`, so nothing in a
  table can be tapped or selected.

Both confirm the marker-hiding and decoration parts of option A below.
Neither covers wrapped, tappable table cells in one text view: mdoc builds
that part itself.

## Round 1 (all recommendations accepted)

1. **Scope: how it looks, not editing.** The text stays read-only. Editing
   on the phone is a later grill; the user expects it to start in the raw
   Markdown view (Q7).
2. **Markers hidden everywhere**, selections included, as desktop does.
   `\_` shows as `_`. The text underneath stays the source.
3. **Architecture A: one TextKit 2 view plus drawn decorations.** Table
   lines are hidden from layout and an embedded table view draws in their
   place; bullets, quote bars, code cards and rules are drawn around the
   text. Starts with a 1–2 day spike on the bilingual 3-column contract;
   if table cells or hidden lines fail there, fall back to **B** (a stack
   of per-block views, each knowing its source offset). C (web view) is
   out.
4. **Tables scroll sideways.** Cell text wraps inside columns of at least
   about 140 pt, scaled with Dynamic Type; stored column widths
   (`<!-- table:… -->`) set the proportions and the table style is
   honoured; bold header with a light fill, thin grid lines, an edge fade
   while more is off screen. Candidates in cells stay tappable; Replace…
   works inside a cell. No per-row cards: they would break parallel
   bilingual columns.
5. **Lists as desktop draws them:** `•` (desktop uses it at every depth;
   the `•` `◦` `▪` asked about was a wrong assumption, corrected in the ADR), `1.` → `a.` →
   `i.` numbering (renumbered, not the source digits), hanging indent, task
   items as ☐ / ☑ that do not toggle. The rules come from the core.
6. **Other blocks:** headings at desktop's relative sizes and spacing,
   quote bars, alert cards with a title, code cards with fences hidden,
   drawn rules, footnote definitions and raw HTML greyed. Math shows as
   monospaced source and an image as an "Image" chip, for now.
7. **A Show Markdown switch** in the document's ⋯ menu shows today's
   source view; off by default, remembered for the app, not per document.
   It is where phone editing is expected to begin later.
8. **Typography:** iOS text styles (Dynamic Type) scaled to desktop's
   heading sizes and paragraph and list spacing; ADR 0087's rules stay (no
   hyphenation, short blank lines, stronger dark fills).
9. **Recognition from the core:** `mdoc-markdown` and a richer
   `markdown_styling` export carry list depth and number, task state, table
   cells and widths, alert kind and the ranges to hide. Android uses it
   later without redoing the rules.
10. **Not rendering, filed separately:** the itinerary PDF imported as one
    paragraph; letter-spaced text reversed on import
    (`a|i r u o N e v e t S`); the prompt-guide table's source has merged
    rows; booking codes (`ATOOU7`, `GLDNBA`) not proposed.

## Round 2 (all recommendations accepted)

The document screen has no ⋯ menu today (only Identities), and the Reply
pane and the share extension's `RestoredTextView` style text with the same
`MarkdownStyler`.

11. **Desktop's rules move into `mdoc-markdown`** and desktop calls them
    from there, as the first PR, with tests that pin desktop's output.
    Writing direction (RTL tables and lists) moves with them.
12. **Spike pass test** on the bilingual contract, in one text view:
    (1) cells wrap and the table scrolls sideways, (2) table source lines
    take no height, (3) a tap on a name in a cell opens its sheet,
    (4) Replace… in a cell hits the right source range, (5) VoiceOver reads
    cells and the Candidates rotor reaches names in cells, (6) smooth
    scrolling on the iPhone 12 Pro with 50 pages and 20 tables. A miss on
    1–4 means option B; a miss on 5–6 is work inside A.
13. **Selections stay inside one cell** or outside tables.
14. **VoiceOver reads tables as data tables** (row, column, header on
    change).
15. **Show Markdown is a checkmark in a new ⋯ menu** beside Identities;
    one app-wide setting.
16. **The Reply pane and the share extension render too** and follow Show
    Markdown.
17. **No full-screen table view** for now; revisit after the survey.
18. **Delivery:** spike in this session, then a PR stack (rules into the
    core; markers, lists, blocks and Show Markdown; tables), synthetic
    survey documents mirroring the five screenshots (no real names, INN or
    OGRNIP), iOS and desktop shots side by side in light, dark and the
    largest text size, a rendering row in `platform-acceptance.md` section
    13, an Android follow-up issue. Codex review before each PR.

After the tree closed the user skipped the separate spike: the
implementation goes straight to the PR stack and must meet the Q12 pass
test itself, falling back to option B if 1–4 fail. Issues filed: #565
(itinerary as one paragraph), #566 (reversed letter-spaced text), #567
(merged table rows), #568 (booking references), #569 (Android).

The tree is closed; [ADR 0096](../adr/0096-rendered-markdown-on-mobile.md)
holds the decisions.
