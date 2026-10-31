# iOS UI polish

Status: Accepted, 2026-10-31.

## Problem

iOS 0.1's screens ([ADR 0085](0085-mobile-apps.md)) work, but a survey of
every screen in light and dark
([before set](../evidence/ios-app/2026-10-31-ui-survey/README.md)) found
layout defects (a gap in the candidate sheet, hyphens inside values, a
wrapping button bar), faint dark-mode highlights, a Home list that repeats
each title, two-step paste and share, and restored replies shown as raw
Markdown.

## Decision

- **Home is a flat list, newest first.** Each row shows the title, the
  case's dot and name, and the status as a symbol with secondary text.
  Status never has a colour of its own, so the case colour is the only
  colour on a row.
- **Review text keeps its source and styles it closer to reading.** Blank
  lines draw at about a third of a line, Markdown escapes dim as markers
  (in `mdoc-markdown`, so desktop gets them too), table rows use a smaller
  monospaced font, and no text is hyphenated.
- **Highlights meet 3:1 in dark mode.** Candidate and alias fills and
  underlines have their own dark colours with at least 3:1 contrast
  against the background.
- **Button bars never wrap a label.** A bar whose labels do not fit on one
  line stacks its buttons, as at accessibility text sizes.
- **Paste is one sheet** (text, title, case). **Share is one tap** to the
  share sheet with the text and the document's title as its preview; a
  long press offers Share as Markdown file.
- **The hand-off bar follows the pane.** On the Reply tab it shares, copies
  or replaces the reply; on the Text tab it acts on the document.
- **Restored replies are styled** with the review screen's spans and
  colours, restored values highlighted, in the app and in Restore with mdoc.
- **An empty scan says so**: a notice and the Home status "Nothing found".
- **The UI survey is a test.** `ScreenshotUITests` takes every screen,
  light and dark, on iPhone 17 and iPad Pro 13-inch, when run with
  `TEST_RUNNER_MDOC_SCREENSHOTS=1`; it is not part of local CI.

## Invariants

- ADR 0085's rules hold: read-only source text, candidate offsets are the
  core's source offsets, every string in the String Catalog, VoiceOver and
  Dynamic Type on every new control, Copy for AI bytes identical to desktop.

## Consequences

- Platform acceptance gains row P16 (no hyphenation, dark contrast).
- Interview and findings: [iOS polish grill](../design/ios-polish-grill-2026-10-31.md).
