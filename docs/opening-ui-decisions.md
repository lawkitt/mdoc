# Sidebar, source opening, and notices

Status: implemented; automated verification passed.

## Confirmed — round 1

- Collapsing the sidebar retains a narrow vertical rail with all document tabs
  and a plus button. Use file-type icons, filename tooltips, a clear active-tab
  highlight, and plus to create a blank Markdown tab. Preserve the current
  elegant visual design.
- Opening a PDF/DOCX creates one paired tab: editable converted Markdown on the
  left and the original preview on the right. Show the preview as soon as ready
  and convert automatically in the background when OCR is not required.
- Converted Markdown is unsaved; Save asks for a `.md` destination. Preserve the
  original source file.
- If any PDF page needs OCR, including mixed PDFs, withhold automatic Markdown
  conversion and elegantly highlight that OCR is required. OCR remains explicit.
  Never silently present incomplete native extraction as complete conversion.
- DOCX embedded images remain images rather than triggering automatic OCR.
- Restyle top and bottom warning ribbons to fit the existing design.

Rationale: opening provides a predictable side-by-side reading/editing flow;
collapsed navigation remains useful; OCR consent and source preservation remain
explicit.

## Relationship to existing contracts

These confirmed opening decisions supersede preview-only/manual-conversion
clauses in `bulk-open-decisions.md` and `local-ocr-decisions.md`. Other existing
tab identity, lifecycle, source preservation, and OCR consent requirements remain
unless explicitly changed in subsequent rounds.

## Confirmed — round 2

- Before conversion completes, use a restrained noneditable left-pane status
  panel. OCR-required state shows affected pages and Run OCR / Set up OCR as
  the primary action; Extract native text only is secondary and explicit.
  Do not show an automatic consent popup.
- Automatically converted but unedited Markdown can close without a save prompt.
  Editing enables normal unsaved-change protection; Save is immediately available.
- Convert on first tab activation, including restored source tabs. Keep conversion
  sequential and automatically wait when another tab is converting. Completion
  never changes the active tab.
- Keep notice positions, with compact rows, existing typography/palette, subtle
  tinted backgrounds, small severity icons, quiet actions, and expandable details.
  Use a warm OCR-required accent and restrained red for failures. Dismissing a
  notice leaves unresolved OCR visibly indicated in its pane.
- Compact tabs preserve drag reordering and close shortcuts; expose close through
  a context menu rather than a second rail button. Keep dirty/busy/error indicators
  subtle.

Rationale: reading should not create a save obligation, background work remains
bounded, and important states remain discoverable without intrusive UI.

## Additional requested scope

- Add a scrollbar for Markdown.
- Allow horizontal scrolling in PDF/DOCX previews.
- When Open replaces an untouched default Untitled.md placeholder, remove that
  placeholder. Preserve blank tabs when the user intentionally created additional
  blank tabs, including the original blank tab in that multi-blank scenario.

## Confirmed — round 3

- Show slim, subtle, draggable scrollbars whenever content overflows: vertical
  for Markdown and horizontal for overflowing PDF/DOCX previews. Match the
  current theme and strengthen the appearance slightly on hover.
- Initially fit previews to width. When zoom makes pages wider than the pane,
  allow horizontal scrolling through trackpad gestures, Shift+wheel, and scrollbar
  dragging. Keep Markdown and preview scroll positions independent.
- Only an automatic, untouched startup/fallback blank tab is disposable. Never
  infer disposability from the filename or current tab count.
- Explicitly creating another blank protects both the original and the new blank,
  even if one is later closed. Explicitly created blanks are never disposable.
- Editing an automatic blank permanently protects it, even if its text is later
  deleted.
- Open removes a disposable blank only after at least one file is successfully
  admitted. Cancellation or an entirely rejected selection leaves it intact.
- Persist the distinction across restart.

Rationale: scrolling stays discoverable and consistent with the existing UI;
opening files clears only disposable placeholders and preserves user intent.

## Final design agreement

The user agreed to all three rounds and then requested implementation. Existing behavior
outside this document's changes remains governed by the earlier contracts.

Implementation fact: current DOCX conversion skips OCR and does not classify
image-only DOCX documents as requiring recognition; this design does not yet
authorize a new DOCX OCR pipeline.

## Implementation and verification

- Collapsed navigation retains format tiles, filename/path tooltips, active and
  status indicators, drag ordering, and context-menu close. The plus button stays
  fixed above the scrollable tab list.
- PDF/DOCX tabs begin with a noneditable left status pane and a right preview.
  Native conversion starts on first activation, including restored source tabs,
  and waits automatically for the shared conversion slot. Hidden unvisited tabs
  remain lazy; results remain in their original tabs.
- OCR requirements retain affected page numbers. Inline actions require explicit
  consent for recognition/setup or native-only extraction. Waiting for consent
  releases the conversion slot. Dismissing the warm notice retains the pane state.
- Untouched automatic conversions close without prompting and restore from their
  source paths, without storing generated Markdown in the session manifest.
  Editing enables normal save protection; Save writes a separate Markdown file.
- Empty blank-tab records persist provenance without document text. Explicit New
  and any editor change permanently protect the automatic placeholder; canceled
  or rejected opening leaves it intact.
- Compact notices use themed severity accents, expandable details, and dismissal.
  Markdown and preview share slim draggable overflow scrollbars. Horizontal PDF
  scrolling supports gestures and Shift+wheel, preserves the vertical offset,
  and updates form/highlight coordinates. Scrollbar drags retain viewer focus.
- Verification on macOS: `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace` passed: **322 passed, 8 ignored**. The existing Cargo
  registry-age/future-compatibility notices remain.
- Added coverage for blank replacement and restart, generated-document save
  protection, lazy conversion queueing and cancellation, inline OCR consent,
  compact navigation/context close, independent scrollbar dragging, Shift+wheel,
  viewer focus, and form/highlight coordinates after horizontal scrolling.
- Native visual approval, native keyboard/trackpad feel, and Windows/Linux
  execution still require platform checks. Headless GPUI tests establish flow
  and geometry behavior, not visual approval.
