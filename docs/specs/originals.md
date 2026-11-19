# Originals

Folds: none.

How mdoc shows a source document beside its Markdown: the DOCX rendering,
its comments and hidden text, and PDF annotations, on desktop and phones.
Terms are in [CONTEXT.md](../../CONTEXT.md). Record: the 2026-11-18
comments grill (#927).

## What comments are

- Comments are context for the original, not part of the Markdown: they never
  enter the Markdown, the saved file, Copy Markdown, Copy for AI or Share.
- Comments are never scanned or pseudonymized and show as written in every
  view, Pseudonymized included, because nothing carries them out of the
  device.
- Cards are read-only: mdoc never writes the source file, so there is no
  reply, resolve or delete.
- No "Copy all comments", because it would carry unscanned names out in bulk;
  a card's text is selectable and ⌘C copies it like the page's text.
- Show comments and Show hidden text are app-wide settings remembered across
  launches, never per document and never written into a document.

## Extraction

- The forks extract comments and hidden content (AnyDoc for office formats,
  pdf-inspector for PDF annotations) as structured data beside the Markdown:
  author, date, text, anchor text, replies and resolved state.
- mdoc keeps no comment parser of its own; the count the note shows and the
  cards the pane shows come from the same extraction.
- Comments are read from DOCX, ODT and RTF, and PDF annotations that carry
  text (notes, FreeText, markup with contents); DOC, PPTX and XLSX comments
  are counted when the fork can, else later.
- A format that drops comments always counts them; nothing is lost without a
  note.

## DOCX rendering

- DOCX renders with docxide, on desktop in the supervised worker and on
  phones in process, under the same input limits.
- docxide is bumped at the start of each desktop milestone with a
  before/after on the DOCX preview corpus and the fidelity check, one PR per
  bump; no fork unless a patch is unavoidable.
- Upstream docxide issues carry synthetic fixtures only, never a user's
  document.
- Bundled fallback fonts reach docxide as its `font_dirs` option, never the
  environment: Carlito for Calibri, Caladea for Cambria, and Liberation
  Sans, Serif and Mono. docxide looks up the requested family first, so an
  installed Calibri or Cambria still wins; the bundle outranks only a
  user-installed copy of the same bundled family.
- The currency-sign fallback stays until a test shows the bundled fonts
  draw ₽ and the newer signs.
- The fidelity check scores docxide against Word-for-Mac reference PDFs of
  about 8 EN/RU legal documents (comments, hidden text, tables, numbering,
  headers and footers) by SSIM and page count; run by hand at each bump, it
  fails on a drop of more than 2 points.
- docxide's own comment pane stays off: it bakes comments into the page,
  shrinks the body to 76% and can't follow the theme.

## Comments in Source

- The render copy wraps each commented run in a link to `mdoc-comment:<k>`,
  `k` indexing the set of comments open over that run; its `/Link`
  rectangles are the anchors, never clickable links, and mdoc draws the
  highlight itself.
- An anchored passage gets a soft, theme-aware tint that stays readable
  under both page tones.
- A wide Source pane shows the **comment rail**: cards beside the page at
  their anchors' height, pushed down when they would overlap.
- A collapsed card shows initials, name, date, two lines of text and "N
  replies"; clicking the card or its passage expands it and strengthens the
  passage's tint.
- A narrow pane, 640×480 included, shows **comment markers** in the page
  margin instead; a marker opens its card under the line, and markers on one
  line show a count.
- Rail or markers follows the pane's width; there is no setting.
- "💬 N" in the Source header shows or hides comments; hidden means the plain
  page with no tint. N counts every comment, replies included, as the
  phones' "Comments N" and the left-out line do.
- A comment with no place on the page (a header, a footer, an unmatched
  range) sits in "Not on the page (N)" at the top of the rail.
- Replies nest under their comment with author, date and text.
- A resolved thread is a quiet "✓ resolved" chip at its anchor, with no
  tint; clicking it expands a dimmed card.
- OCR reads a scanned page rendered without comment annotations (notes,
  FreeText, popups, markup appearances), so their text never reaches the
  Markdown; stamps, ink and form widgets stay drawn, because they are part
  of the page.
- PDF annotations use the same rail and markers at the annotation's
  rectangle; mdoc tints highlight and underline areas and hides the PDF's own
  note icons and popups so nothing draws twice.

## Hidden text in Source

- The render copy lifts `w:vanish` and draws hidden text in place with a
  dotted underline and muted ink, as Word does with hidden text shown.
- "Hidden text (N)" in the Source header shows it; the control appears only
  when the document has hidden text and is off by default, so the page
  matches what Word prints.
- PDF text no reader sees (invisible, or drawn in its background's colour or
  covered) and form field names are counted only, because they are mostly
  OCR layers and names like "Text1".

## The import note

- A partial import (OCR pages skipped) stays a notice on desktop and a banner
  on phones.
- Everything else the Markdown leaves out is the **left-out line**: an ⓘ in
  the document header (desktop) or under the title (phones) listing counts,
  never orange.
- Comments and hidden text with a rendering count on their Source controls,
  not in the note.
- A format with no rendering counts its comments in the left-out line ("3
  comments not shown"); a format that gains a rendering gains the rail.

## Phones

- Phones render DOCX in process into the Original pane, after a measurement
  on the iPhone 12 Pro (time and peak memory on the legal-opinion and
  coverage fixtures and a 100-page DOCX) stays within the budgets; if it
  doesn't, phones keep the counts and a Later line.
- On a phone comments are margin markers; a tap opens a bottom sheet with
  the thread; iPad in landscape shows the rail.
- Android follows as it reaches the Original pane.

## Accessibility

- The rail or the markers are the "comments" F6 region; Tab walks cards in
  page order, Enter expands or collapses, and Esc returns to the page.
- A card's label reads "Comment by <author>, <date>, on '<passage>': <text>.
  N replies."
- Next and previous comment are rows in `src/shortcuts.rs`; they scroll the
  page and focus the card.
- Markers are labelled buttons.
- The active passage is underlined as well as tinted, so colour is never the
  only cue; the tints and card colours are in `style::contrast_tests` for
  both themes and page tones.
- Scrolling to a comment honours Reduce Motion.
- On phones markers are VoiceOver and TalkBack elements right after their
  line, iOS adds a "Comments" rotor, and the sheet reads the thread in order
  at every Dynamic Type size.
