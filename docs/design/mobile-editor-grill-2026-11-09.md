# Mobile Markdown editor grill, 2026-11-09

Interview record for a new milestone: a rich Markdown editor in the mobile
app, with Obsidian's iOS editor as the target feel. Decisions go into a new
ADR (and an amendment to [ADR 0085](../adr/0085-mobile-apps.md)'s "no full
Markdown editor") once the tree is closed; this file keeps the findings,
the answers and the open branches.

## Starting point

- **Today the phone text is read-only.** ADR 0085 ("It has no full
  Markdown editor"; review screen "No text editing"), ADR 0096 ("The text
  stays read-only"; phone editing "is expected to start in" the Show
  Markdown view) and ADR 0098 ("Never: find and replace, or any other
  editing"). Rules live in the Rust core (ADR 0104); saved bytes are the
  text as typed (ADR 0030, ADR 0060).
- **The iOS text stack** is one TextKit 2 `UITextView` per document
  (`Shared/UI/RenderedMarkdown.swift`, 1,166 lines; `MarkdownStyler`, 349;
  `MarkdownTextView`, 644): markers hidden with a 0.01 pt clear font,
  marker-only lines folded, tables drawn by an embedded grid of cell text
  views, the whole attributed string rebuilt on each text revision. Android
  has a source-view `TextView` with dimmed markers (`ReviewText.kt`, 397
  lines), not rendered yet (#569).
- **The core already follows free edits.** `mdoc-ffi`'s
  `document_session/review.rs` (`commit_edits`) and `review/history.rs`
  re-implement the desktop editor's linear history (256 steps,
  replace-ranges, transactions into `mdoc_pii::Review::on_transaction`),
  and `mdoc-pii` moves candidates through arbitrary edits, because desktop
  allows free editing.

## Phone survey (iPhone 17 Pro, 2026-11-09)

Driven by a throwaway XCUITest bundle on the phone (no iPhone Mirroring):
`XCUIApplication(bundleIdentifier:)`, scripted taps and typing, screenshots
and accessibility dumps. One scratch note per app, deleted afterwards.
Screenshots: [evidence](../evidence/mobile-editor/2026-11-09/).

**Obsidian 1.15** (CodeMirror 6 in a web view, so not native text)
- *Live preview is the default* of three modes (live preview, source,
  reading). Markers are hidden; **the span the caret touches shows its
  markers dimmed** (`**bold**`, `*italic*`, `[link](…)`), a heading shows
  its `#` and a quote its `>` while the caret is on the line. Revealing
  reflows the line (a word jumped to the next line).
  ([1](../evidence/mobile-editor/2026-11-09/obsidian-1-inline-reveal.jpg),
  [6](../evidence/mobile-editor/2026-11-09/obsidian-6-bold-reveal.jpg),
  [7](../evidence/mobile-editor/2026-11-09/obsidian-7-heading-reveal.jpg))
- *Typing helpers:* `[` auto-pairs and a typed `]` steps over the pair; a
  code fence closes itself; Return continues bullets, tasks and numbers
  (+1) and ends the list on an empty item.
- *Blocks:* round checkboxes, dot bullets, grey numbers, a purple quote
  bar, a code card with syntax colours and a language label once the caret
  leaves ([4](../evidence/mobile-editor/2026-11-09/obsidian-4-rendered.jpg)).
- *Links* render with an external-link icon; **a tap opens the URL**
  (Safari), it does not place the caret.
- *Tables are a grid widget* the moment a header row ends with Return:
  cells, drag handles, + buttons for a row and a column. Everything typed
  after went into cells, and **the widget rewrote the whole table source**:
  padded columns, typed pipes escaped as `\|`
  ([5](../evidence/mobile-editor/2026-11-09/obsidian-5-table-widget.jpg),
  [9](../evidence/mobile-editor/2026-11-09/obsidian-9-source-mode-table-rewritten.jpg)).
- *Toolbar:* a floating Liquid Glass capsule above the keyboard, paged by
  swiping: undo, redo, wikilink, file, tag, attach, heading, bold, …,
  quote, comment, link, bullet, numbered, task, indent, outdent, customize;
  a separate round button hides the keyboard
  ([8](../evidence/mobile-editor/2026-11-09/obsidian-8-selection-toolbar2.jpg)).
- *Selection* uses the native handles and the stock iOS edit menu (Cut,
  Copy, Paste, Replace…, Writing Tools, Look Up, Translate); no formatting
  items in it.
- *Source mode* keeps styles (bold is bold, headings large) with every
  marker shown dimmed.

**Bear 3.0** (native)
- Markers hidden; **the bold span under the caret shows `**` dimmed**, as
  Obsidian. The title's `#` stays visible. No auto-pair for `[`. A link
  shows a pencil chip to edit it
  ([3](../evidence/mobile-editor/2026-11-09/bear-3-bold-reveal.jpg)).
- Typed table source becomes a grid as soon as the separator row is
  complete; typed cell text stayed correct
  ([1](../evidence/mobile-editor/2026-11-09/bear-1-blocks-table.jpg)).
- *Toolbar:* a small bar (undo, redo, tag, hide keyboard, attachment,
  **BIU**). BIU **swaps the keyboard for a formatting keyboard**: jump to
  top/bottom, photo, drawing, table, date, spacing; H1, B, I, U, S; list,
  highlight, code, link; indent, outdent, task, rule, footnote
  ([2](../evidence/mobile-editor/2026-11-09/bear-2-format-keyboard.jpg)).
  **Inside a table it becomes a table keyboard**: add a row above/below, a
  column left/right, delete a row or column, move between cells, align
  ([4](../evidence/mobile-editor/2026-11-09/bear-4-table-keyboard.jpg)).
- Selection: native handles and the stock edit menu.
- XCUITest never saw Bear idle (each step waited 60 s): it animates
  something all the time.

**Taio 1.68** (native)
- **A source editor**: all markers visible and coloured, bold drawn bold,
  a Ln/Col chip, plus a separate **Preview** (an HTML page, `.html` in the
  tab) ([1](../evidence/mobile-editor/2026-11-09/taio-1-source-editor.jpg)).
- A formatting keyboard like Bear's (indent, B/I/U/S, heading, image,
  link, table, move line up/down, list, quote, rule, code, math) and a `/`
  command menu (Tag, Format, More Commands)
  ([2](../evidence/mobile-editor/2026-11-09/taio-2-format-keyboard.jpg),
  [3](../evidence/mobile-editor/2026-11-09/taio-3-slash-menu.jpg)).
- Pitfall seen: Return on an empty `>` line ended the quote and ate the
  blank line, so the table typed next became quote text by lazy
  continuation ([4](../evidence/mobile-editor/2026-11-09/taio-4-preview-lazy-quote.jpg)).

**Common ground:** all three continue lists on Return, use native
selection and the stock edit menu, keep a formatting bar above the
keyboard, and close code fences. Obsidian and Bear hide markers and reveal
the span under the caret; neither has tables in source form while editing.

## Open-source editors read (2026-11-09)

Read as technique references; clones were not built or run.

| | FSNotes (MIT) | NotesQuick (no licence) | MarkdownEditorKit (MIT) | swift-markdown-view (MIT) |
|---|---|---|---|---|
| Kind | App, iOS + macOS | App | Library | Renderer + editor library |
| Text stack | TextKit 1 (`NSLayoutManager` delegate) | `UITextView` | `UITextView` (source) + `WKWebView` contentEditable (rich) | TextKit 2, asserted |
| Parser | Own regexes (Marklight); cmark-gfm only for the web preview | Regexes | Regexes + own HTML converter | Own UTF-16 scanner (editor) + swift-markdown (renderer) |
| Hides markers | Yes: `.null` glyphs; reveal per paragraph while focused | No (dimmed) | Only in the lossy web mode | Yes: 0.1 pt + clear + negative kern; reveal on the selection's lines |
| Bytes kept | **No**: tasks and images become attachment characters, rebuilt and normalised on save | Yes | Source mode only | Yes |
| Undo-safe | No (edits bypass undo) | No (`text =` resets undo) | No (`attributedText =`) | Yes (`replace(_:withText:)`) |
| IME-safe | – | No | No | Yes (skips rules while marked text) |
| Restyle | Edited paragraph, but whole-document scans per key | Whole document | Whole document | Whole document, also on caret moves |
| Tests | None | None | Formatter | ~475 + snapshots |

Takeaways for mdoc:
1. **One parser.** Every project with two parsers (editor vs preview) has
   them disagree. mdoc already has one: `mdoc-markdown` in the core.
2. **Never replace the text to restyle.** Change attributes in place inside
   `beginEditing`/`endEditing`; route every programmatic edit (toolbar,
   list continuation, paste) through `replace(_:withText:)` so it lands on
   the undo stack, and skip rules while there is marked text.
3. **The storage string is the file.** FSNotes swaps `- [ ]` and images
   for attachment characters and loses bytes on save; draw checkboxes,
   images and bars as decorations instead (mdoc's renderer already does).
4. **Restyle incrementally** (the edited block plus the lines the caret
   left and entered); all four restyle too much per keystroke.
5. **Pure commands, thin UI.** MarkdownEditorKit's
   `MarkdownFormatter.apply → (text, selection)` and swift-markdown-view's
   `InputRule`/`TextChange` with `mapOffset(bias:)` are testable without a
   text view; for mdoc they belong in Rust (ADR 0104).
6. **Toolbar host:** an `inputAccessoryView` hosting controller that
   refuses first responder, with active-state buttons
   (MarkdownEditorKit).

## Our editor crate (`crates/mdoc-editor`)

There is no crate named `markdown-editor`; the desktop editor is
`mdoc-editor` (GPUI, about 15.7k lines without tests, derived from
Zorite's editor), on `mdoc-markdown` (recognition) and `mdoc-history`
(edit transactions).

- **GPUI cannot run on a phone.** The pinned `gpui-pre` 0.3.2 has
  backends for macOS, Windows, Linux/FreeBSD and wasm only.
- **About a quarter is shareable logic.** Already GUI-free: `search.rs`,
  `spell_exclusions.rs`, `transactions.rs` (~860 lines). Pure logic with
  GPUI only in its signatures (~2.8k lines): Enter/⇧Enter rules
  (`enter_edit`, `continued_item`, `line_break`), paragraph joins on
  ⌫/⌦ (`break_delete_range`, `delete_break`), formatting toggles
  (`toggle_wrap_pair`, `unwrap_edit`), indent/outdent, paste
  normalisation, the coalescing undo stack, `turn_into` (11 block kinds),
  task toggle, ordered renumbering, word movement, and the table commands
  (`tables.rs` 600–1160). The rest (~8.5k lines) is layout, painting,
  pointer handling and IME glue, which a phone rewrites natively anyway.
- **Catch:** several "pure" rules read last-paint state (`table_rows`,
  `offset_maps`, `wrap_rows`); extracting them means re-deriving that from
  `mdoc_markdown::tables::table_regions` and friends.
- **Desktop's reveal rule differs from Obsidian's:** formatting markers
  (`**`, `*`, `~~`, `` ` ``, `<u>`, `<mark>`) **never** reveal under the
  caret (test `formatting_markers_never_reveal`); links and other
  structural constructs do; a selection reveals inline markers but not
  line prefixes.
- **No auto-pairing and no link insertion** exist on desktop today.
- **`mdoc-ffi` exports** one whole-document `markdown_styling(text)` (UTF-8
  ranges; spans, line kinds, list items, blocks, tables, images) and
  `pasted_markdown`. `mdoc-markdown` is not incremental (inline scans are
  per line; block scans whole-document); desktop caches per revision.
- **Ways to use it:** (a) GPUI on the phone: impossible. (b) Extract a
  GUI-free editor core (model, selection, history, Markdown commands,
  table commands) that desktop and both phones call through UniFFI, with
  native rendering and input. (c) Share only spans and write commands in
  Swift: breaks ADR 0104 and doubles Android. (d) (b) staged, starting with
  editing in Show Markdown, where display offsets equal source offsets.

## Design tree

Root: **what the editor is for** (Q1). It decides the editing surface,
how editing meets review, toolbar contents and table depth.

- Purpose and documents (Q1) → where editing happens and its entry point;
  how a caret tap coexists with candidate taps; what happens to proposals
  and decisions on edit; save model; Copy for AI with unsaved edits.
- Platforms (Q2) → Android timing and the shared-core surface.
- Architecture (Q3) → core extraction scope, FFI shape (per-keystroke vs
  structural calls), incremental styling, UTF-8↔UTF-16 mapping, IME.
- Reveal rule (Q4) → caret affinity at hidden boundaries, desktop parity.
- Later rounds: tables (grid vs source, byte rules), toolbar and
  formatting keyboard, auto-pair and typing helpers, links (tap opens vs
  caret), images and attachments, find and replace, spellcheck, undo
  surface, iPad hardware keyboard, accessibility, performance budgets,
  acceptance and milestone slicing.

## Round 1 (all recommendations accepted)

1. **Purpose: edit any document, as desktop does.** Imported, scanned,
   pasted and new blank documents are editable; review keeps working over
   the edits (proposals follow them, as `mdoc-pii` already does for
   desktop). Not a notes app: no tags, wikilinks or vault. Amends ADR
   0085's "no full Markdown editor" and the "No text editing" bullets of
   ADR 0085, 0096 and 0098.
2. **iOS first.** Android uses the same core in this milestone; its editor
   screen is its own Android roadmap item, after its rendered Markdown
   (#569).
3. **Architecture A: native TextKit 2 plus a shared Rust editor core.**
   The rendered text view becomes editable. The editing rules move out of
   `mdoc-editor` into a GUI-free core crate that desktop also switches to,
   so each rule exists once (ADR 0104). Plain typing is applied natively
   and reaches the core as coalesced edits; structural keys (Enter, Tab,
   ⌫ at boundaries, formatting toggles, paste) call the core directly.
   Rejected: CodeMirror 6 in a web view (second parser, rules outside the
   core, web accessibility) and Swift-only rules (breaks ADR 0104, doubles
   Android).
4. **Reveal rule: Obsidian's, on the phone.** The span the caret touches
   shows its markers dimmed; line prefixes (`#`, `>`, list markers) show
   on the caret's line. The core takes the rule as an option; desktop
   keeps "formatting markers never reveal" for now (its own question
   later).

## Round 2 (all recommendations accepted)

Facts behind it: desktop saves Markdown files explicitly (unsaved changes,
at-risk tabs, Confirm dialog); mobile documents live in the app's own
storage and core saves already run after taps pause (ADR 0104). After an
edit `Review::on_transaction` moves candidates and sets
`awaiting_discovery`; desktop then runs discovery (more mentions of known
identities) in the background (`src/pii/ui/discovery.rs`). Discovery
finds no new names: that takes recognition.

5. **An Edit button, not always-editable.** The review screen switches to
   edit mode: keyboard and toolbar, underlines dimmed and not tappable,
   review bar hidden; Done returns to review. A new blank document opens
   in edit mode.
6. **Show Markdown is editable too, and is the first slice.** Edit mode
   works in both views; the first slice edits with Show Markdown on
   (display = source), proving core edits, undo, proposals following
   edits and saves before live preview adds hidden markers and cells.
7. **Autosave, no Save button:** after typing pauses (~1 s), on leaving
   the document, on going to the background. Undo and redo in the toolbar,
   the core's 256 steps for the open session. No version history.
8. **Discovery runs after each typing pause; recognition only on demand**
   ("Find names again"), never on its own.
9. **Toolbar:** a capsule bar (undo, redo, bold, italic, list, task,
   indent, outdent, Aa, hide keyboard). Aa swaps in a formatting keyboard
   (Turn into H1–H3, quote, code, callout; strike, code, highlight, link,
   rule, table) that becomes table controls inside a table. The selection
   menu stays the system one.
10. **Tables: cells edited in place in the drawn grid.** Typing changes
    only that cell's bytes; row and column actions are the core's table
    commands (desktop's); nothing pads or realigns. In this milestone,
    after the live-preview slice.
11. **Typing helpers:** Return continues and ends lists, tasks, numbers
    and quotes; code fences close themselves. No auto-pair of `[`, `(`,
    `` ` ``; smart quotes and dashes off; system autocorrect and dictation
    on.
12. **Desktop's reveal rule stays** for this milestone; a ROADMAP "To
    grill" item decides after phone use.

## Round 3 (all recommendations accepted)

Facts behind it: the review text's edit menu offers only Replace… (Hide
name) on purpose: "text leaves mdoc through Copy for AI and Share, never a
plain Copy, Share, Look Up or Translate of the selection"
(`MarkdownTextView.swift:523`). Writing Tools and data detectors are off
in all three text views.

13. **In-app clipboard.** In edit mode Cut and Copy go to a pasteboard only
    mdoc reads; Paste reads it first, then the system clipboard. Look Up,
    Translate, Search Web, Share-from-selection and Writing Tools stay
    off; plain text leaves only through Copy for AI and Share.
14. **Edits over names behave as on desktop:** an edit overlapping an
    applied alias stops tracking that mention (it won't restore; other
    mentions keep their identity); an edit inside a pending candidate drops
    it and discovery may propose it again. Edit mode tints applied aliases;
    the first broken alias shows a one-line hint. Copy for AI and Share
    work in edit mode on the current text.
15. **Links in edit mode:** a tap places the caret; with the caret in a
    link a chip offers Open and Edit; the Link button opens a text + URL
    sheet. Review mode unchanged.
16. **New document:** first in Home's + menu (⌘N on iPad unless the
    sidebar's ⌘N covers it); opens in edit mode, empty. Named after its
    first line (without `#`) until renamed; an untouched empty document is
    discarded on leaving.
17. **Paste, images, find, spelling:** rich text and HTML paste through
    `pasted_markdown` (ADR 0105); existing images show and can be deleted,
    no inserting; the system find bar in both modes, Replace only in edit
    mode as core edits; system spellcheck in edit mode only.
18. **Hardware keys:** ⌘E edit mode; ⌘B, ⌘I, ⌘K; ⌘Z, ⇧⌘Z; Tab/⇧Tab
    indent and outdent (next/previous cell in a table); ⌘F; listed in the
    ⌘-hold menu.
19. **Milestone:** ROADMAP iOS item "Editing", iOS 0.3, after the freeze
    (items 13–19). Slices, one PR each: (1) editor core extracted, desktop
    switched, no behaviour change; (2) edit mode with Show Markdown on:
    autosave, undo, proposals following, clipboard, paste, find; (3)
    live-preview editing: reveal, caret over hidden markers, incremental
    restyle; (4) toolbar, formatting keyboard, links; (5) tables; (6) new
    document, Find names again, iPad keys. Device checks after 3 and 6.
20. **Acceptance:** no dropped frame per keystroke on the iPhone 12 Pro in
    the 37-page `legal-opinion.docx` fixture; restyle under 8 ms a
    keystroke, structural core calls under 16 ms (ADR 0104); a typing fuzz
    test keeps saved bytes equal to the view's string; VoiceOver editing,
    hidden markers not spoken, labelled formatting keys, Dynamic Type to
    AX sizes, `performAccessibilityAudit` in edit mode; the extracted core
    passes desktop's editor tests.

## Round 4 (all recommendations accepted)

Facts behind it: the core keeps one line of states for undo (ADR 0098,
`review/history.rs`, 256 steps as desktop's `UNDO_LIMIT`), each named for
the review bar's toast (Replace, Keep, Replace all, Same as…); desktop's
editor keeps text edits and review decisions in the same line.

21. **One undo line.** Typing joins the core's undo line as coalesced
    "Typing" steps beside review decisions, as on desktop; edit mode's
    toolbar undo and review mode's review bar Undo both undo the latest
    step, and the toast names it ("Undid typing", "Undid Replace").
22. **No editing during a job.** Edit is disabled while OCR or recognition
    runs for that document, saying why ("Recognizing…"); Find names again
    starts only outside edit mode.
23. **Replies stay read-only.** The Reply view and Restore with mdoc show
    output; "Save reply as document" goes to ROADMAP Later.
24. **Original beside the text:** the iPad keeps the Original side by side
    in edit mode; on iPhone the Text/Original switch stays, and Original
    hides the keyboard without leaving edit mode.
25. **No new settings.** Show Markdown stays the one display setting; the
    reveal rule and helpers are fixed. The sample document is editable and
    Try a sample restores it.

## Closed

The frontier is empty after round 4 (2026-11-09). Decisions:
[ADR 0109](../adr/0109-mobile-markdown-editor.md); terms in `CONTEXT.md`
(Edit mode, Editor core, Reveal rule, Formatting keyboard, In-app
clipboard); ROADMAP iOS item 23, Android item 10, and two Later lines.

## Follow-up (2026-11-10)

26. **⌘N opens a new document.** Slice 6 (#772) added New document while
    ⌘N still went to Start under the menu label "New document" (ADR 0102).
    The maintainer chose to give ⌘N to New document; Start stays a tap away
    in the sidebar. ADR 0102's Keys bullet and ADR 0109's hardware keys
    are amended.
