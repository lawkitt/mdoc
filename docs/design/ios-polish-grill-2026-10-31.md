# iOS UI polish grill, 2026-10-31

Interview record for the iOS app's UI polish pass, the next step after iOS
0.1's code landed (ADR 0085). The survey of every screen, light and dark, is
in [`docs/evidence/ios-app/2026-10-31-ui-survey`](../evidence/ios-app/2026-10-31-ui-survey/README.md);
numbers in brackets below are its shot numbers. Decisions land in an ADR
once confirmed; this file keeps the findings and the open design tree.

## Rules the polish keeps (ADR 0085)

Read-only text, never re-rendered Markdown: candidate offsets are the core's
source byte offsets. Every string in the String Catalog. VoiceOver on
candidates, the candidate sheet and Identities; Dynamic Type; light and
dark. Copy for AI bytes identical to desktop. No network but model
downloads.

## Findings

### Defects

1. **Candidate sheet: empty band under Category** [14, 15]. The Found and
   Category section is about twice its height, so at the medium detent the
   decision footer and Next proposal fall below the fold. The title repeats
   Found and truncates; Next proposal shows when nothing is left to review.
2. **Hyphens inside values** [17, 30]. TextKit hyphenates long words:
   Identities shows `example.-com`, and at large text the review shows
   `anna.smirnova@ex-ample.com`, a hyphen inside a candidate.
3. **Hand-off bar wraps** [16, 24]. "Paste reply" breaks onto two lines and
   the three buttons differ in height on a 402 pt iPhone at default text.
4. **Setup card has no card in light mode** [01, 08, 11]. Its
   `.background.secondary` is the grouped list's own colour, so the card
   floats as loose text; dark mode and the iPad sidebar show the card.
5. **No app icon** [21]. The share sheet shows the blank placeholder for
   mdoc and Restore with mdoc (known gap: `build/appicon.svg` exists).

### Visual polish

6. **Blank Markdown lines make tall gaps** [06, 12, 26]. Each source blank
   line takes a full body line plus paragraph spacing, so paragraphs sit
   far apart and a phone screen shows little text.
7. **Converter escapes show** [12, 22]. DOCX lists arrive as `1\.`; the
   backslash is drawn as text, not dimmed as a marker.
8. **Tables wrap as pipe rows** [12, 28]. Long rows wrap mid-cell.
9. **Dark-mode highlights are faint** [16, 24, 28]. The alias fill
   (`EMAIL_1`) and candidate fill are dark teal on black.
10. **Home repeats the title** [08, 11]. Each new case is named after its
    document, so the case header and the row say the same thing. The case
    dot (orange, yellow) competes with the orange "to review" status; the
    yellow dot is faint on light grey.
11. **Two blue buttons** [07]. The document's setup card adds a prominent
    "Download 649.5 MB & continue" right above the prominent Find names.
12. **Identities** [17]. No Done button; the "Proposed · 1" state is small
    grey-blue text with a dashed circle.
13. **Settings shows model file names** [09]: "GLiNER2 PII FP16 (Detector)",
    "PP-OCRv5 Cyrillic + v6 detector (English & Russian)".

### Flow

14. **Paste is two sheets** [03–05]: Paste text, Next, then Add document
    for the title and case. The editor has no placeholder.
15. **Share is a menu, then the share sheet** [20, 21]: two taps for the
    common case; the share preview starts with the placeholder note rather
    than the document's title.
16. **Restored text is raw Markdown** [22, 24]: the reply pane and the
    extension show `##` and `**` as plain text, unlike the review screen.
17. **Reply pane labels** [24, 25]: the Reply segment and the Reply menu
    share a name; the bottom bar keeps Share and Copy (of the document)
    while the reply shows, so which text they act on is unclear.
18. **No "nothing found" moment** [26]: a document with no candidates
    goes straight to Ready to share, with no word that the scan ran.
19. **Reply sheet wording** [23]: "Aliases are restored with the names in
    “Settlement agreement”." mixes alias, name and case.

### Outside UI polish

- Rules-only scans (devices below the detector floor) found the emails
  but not `07700 900461`, `01632 960 418` or the IBAN
  `GB82 WEST 1234 5698 7654 32` [12, 28]: those rules need a label word
  nearby. The rules-only setup card promises phone numbers and IBANs.

## Facts found while grilling

- `mdoc_ffi::markdown_styling(text)` returns spans for any text, so a
  restored reply can be styled by the same core spans as the review screen.
- On iOS 26 a text-style font (`preferredFont`, and SwiftUI `Text`)
  hyphenates a word too long for its line, whatever the paragraph's
  hyphenation settings; a plain system font of the same size does not
  (probe on the iPhone 17 Pro Simulator, 2026-10-31). So "no hyphenation"
  means plain system fonts sized from the reader's text size.
- Case colours are the core's eight swatches (a hue per case, shared with
  desktop), and the glossary says a case colour never encodes state.

## Open design tree

Round 1 (answered 2026-10-31): scope, the hand-off bar, the app icon,
automated survey screenshots, committing this survey, the rules-only gap.
Round 2 (answered 2026-10-31): blank lines, escapes, tables, dark
highlights, Home, the document setup card, Identities, Settings names, the
paste sheet, one-tap Share, styled replies, the reply pane, the empty scan,
survey devices. Round 3 (answered 2026-10-31): PR shape, ADR, wording,
acceptance, where the work runs, the rules-only gap. The tree is closed;
behaviour decisions are in [ADR 0087](../adr/0087-ios-ui-polish.md).

## Decisions

### Round 1

- **Scope: defects, visual polish and flow** (findings 1–19). The core
  escape change (7) and tables (8) are decided in round 2.
- **Hand-off bar: one line, stacked when it does not fit.** Share, Copy and
  Paste reply keep full labels on one line; when they do not fit (narrow
  phone, large text) the bar stacks them, as it already does at
  accessibility sizes. Invariant: no label wraps inside a button.
- **App icon in this pass**, made from `build/appicon.svg`, for the app and
  the share sheet's app row.
- **The survey becomes a UI test.** `ScreenshotUITests` grows to take every
  screen in this survey, light and dark, with one command; sub-second
  progress states are left out.
- **This survey is committed** as the "before" set, in the same change as
  the "after" set, downscaled like the other `docs/evidence/ios-app` sets.
- **The rules-only gap is a separate task** in `mdoc-detect`. Until it
  lands, the rules-only setup card stops promising phone numbers and IBANs.

### Round 2

- **Blank lines draw at about a third of a line.** Paragraph style only;
  the text and every source offset stay the same.
- **Backslash escapes are markers.** `mdoc-markdown` reports a Markdown
  escape's backslash as a marker span, so iOS and desktop both dim it.
- **Table rows use a smaller monospaced font** so columns line up and wrap
  less; the source text is unchanged (no grid: that would re-render).
- **Highlights get dark-mode colours** in `Palette.swift`, with at least
  3:1 contrast for the candidate and alias fill and underline against the
  background.
- **Home is a flat list, newest first.** Each row shows the title, the
  case's dot and name, and the status. Status is a small symbol with
  secondary text, never a colour of its own, so case colours stay the
  only colour on the row (the glossary's "never encodes state").
- **The document's setup card is the only prominent button** while it
  shows; Find names returns as the primary action once models are in or
  the card is dismissed.
- **Identities gets Done on iPhone and labelled state chips** (Proposed,
  Replaced, Kept); the chip text is the VoiceOver value.
- **Settings names models plainly** ("Name detector", "Text recognition,
  English and Russian") with the model's own name as a second line; the
  app maps model kinds to names, the core's offers are unchanged.
- **Paste is one sheet**: editor with a placeholder, then title and case,
  then Add. Files, Photos and Scan keep the title-and-case sheet.
- **Share is one tap** to the share sheet with the text; a long press on
  Share offers Share as Markdown file. The share preview shows the
  document's title.
- **Restored replies are styled** with `markdown_styling` spans and the
  review screen's colours, restored values highlighted on top, in the reply
  pane and in Restore with mdoc (`RestoredTextView`).
- **The hand-off bar follows the pane.** On the Reply tab it offers Share
  reply, Copy reply and Paste another; the reply's menu keeps only Remove
  reply.
- **An empty scan says so**: a notice "No names found. Select text to mark
  one." and the Home status "Nothing found".
- **The survey covers iPhone 17 and iPad Pro 13-inch**, on demand, not in
  local CI.

### Round 3

- **Four PRs, in order:** (1) defects: candidate sheet, no hyphenation,
  one-line bar, setup card background, app icon, survey test; (2) escapes
  in `mdoc-markdown` with desktop tests, in its own session; (3) visual:
  blank lines, tables, dark highlights, flat Home, document setup card,
  Identities, Settings names; (4) flow: one-sheet Paste, one-tap Share,
  styled replies, bar follows the pane, empty scan, wording, and the
  before and after survey sets.
- **ADR 0087** records the behaviour decisions; ADR 0085 links it.
- **Wording:** the reply sheet says "mdoc puts back the names from the
  case “…”."; the rules-only card says "This iPhone has too little memory
  for the name detector. mdoc still finds email addresses, and phone
  numbers, IBANs and other details next to their labels; you mark names by
  selecting them." (iPad on an iPad); the candidate sheet's title is the
  category.
- **Acceptance:** the after survey reviewed beside the before set; P12 and
  P13 pass in the Simulator; new row P16 (no hyphenation at any text size,
  dark highlights at 3:1) passes. P14 stays the maintainer's.
- **Where it runs:** this session on macbook-air, one iOS PR at a time on
  its own branch; the escapes PR in a parallel session. Codex reviews each
  diff before its PR. The maintainer is away for this run and allowed work
  beyond the polish scope; the after survey is self-reviewed and left for
  the maintainer.
- **The rules-only gap** goes on the ROADMAP now and gets its own session
  and grill.
