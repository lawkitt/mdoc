# iOS UI survey, 2026-10-31

Every reachable screen of the iOS app, light and dark, before and after the
UI polish ([ADR 0087](../../../adr/0087-ios-ui-polish.md)); findings in the
[grill record](../../../design/ios-polish-grill-2026-10-31.md).

- **`before/`**: `main` at e17f217, taken by hand with `simctl` on the
  iPhone 17 and iPhone 17 Pro Simulators (both 402 × 874 pt, iOS 26.1) and
  the iPad Pro 13-inch (M5). Shots 01–10 run the app as a user would
  (detector tier, no models); 11 onward with `-mdoc-ui-testing` (rules only,
  a seeded fixture).
- **`after/`**: the polish branches, taken by `SurveyUITests`
  (`just ios-survey <folder> [device]`) on the iPhone 17 Pro and, prefixed
  `ipad-`, the iPad Pro 13-inch (M5). The same numbers name the same
  screens; 05 (Paste's second sheet), 20 (the Share menu) and 25 (the
  reply's menu) are gone with them, and the iPad set has its own Home
  (`ipad-01`, `ipad-08`) in place of 31.
- **`layout/`**: the layout pass after it (portrait iPad, one-row hand-off
  bar), the shots that changed, by the same test: `16` and `24` on the
  iPhone 17 Pro; on the iPad `ipad-24`, `ipad-26-pdf-text-original`,
  `ipad-26-pdf-text-identities` (the picker's Identities) and
  `ipad-26-pdf-landscape`.

Both sets are downscaled to 1000 px high. The table names the screens:

| # | Screen | Fixture |
|---|---|---|
| 01 | Home, empty, setup card (detector tier) | none |
| 02 | Add menu | none |
| 03–04 | Paste text sheet, empty and filled | Settlement agreement.md |
| 05 | Add document (title and case) | |
| 06 | Document before Find names | |
| 07 | Document, Find names needs setup | |
| 08 | Home with one document | |
| 09–10 | Settings, About | |
| 11 | Home, rules-only setup card, 1 to review | Settlement agreement.docx |
| 12–13 | Review: top, underlined email | |
| 14–15 | Candidate sheet, medium and large | |
| 16 | After Replace: Share, Copy, Paste reply | |
| 17 | Identities | |
| 18 | Selection with Replace… (light only) | |
| 19 | Replace sheet for a selection | |
| 20–21 | Share menu, share sheet | |
| 22 | Restore with mdoc extension | |
| 23 | Paste AI reply sheet | |
| 24–25 | Restored reply pane and its menu | |
| 26–27 | PDF: Text and Original | tests/fixtures/import/text.pdf |
| 28–29 | Markdown-rich review, top and end | tests/fixtures/mobile-markdown/review.md |
| 30 | Review at Accessibility XL text | |
| 31 | iPad Home, no document open | text.pdf |

Not captured in either set: job progress and Cancel (import and scan finish in under a
second), the interruption banner and OCR resume (see `2026-10-31-ocr-resume`),
the cellular question, error alerts, the camera, Photos and Files pickers,
and the extension's case picker. The before set has no iPad document
layout; the after set's `ipad-26` shows text, original and Identities side
by side, and `layout/` the portrait and landscape layouts that replaced it.

The `¬£` in the before set's shots 24–25 comes from `simctl pbcopy` reading the test reply
without a UTF-8 locale, not from the app.
