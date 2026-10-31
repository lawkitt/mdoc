# Android UI survey, 2026-10-31

Every screen of the Android app ([ADR 0089](../../../adr/0089-android-app.md))
in light and dark, and the review text and Settings at the largest font
(scale 2), on the API 36 emulator as a Pixel 8-sized phone and a Pixel
Tablet-sized tablet, taken by `SurveyTest` with
`just android-survey <folder> phone|tablet`. The app is told the phone has
16 GB, so name finding and its setup card show; the document is the
settlement agreement with its names proposed (`seedProposals`), the scan is
`english.pdf` before text recognition. Downscaled to 1000 px.

[`before/`](before/) is the first survey, of the app as built through
lawkitt/mdoc#319; [`after/`](after/) is the same walk after the polish
round. The tablet sets are prefixed `tablet-`.

| # | Screen |
| --- | --- |
| 01 | Home: setup card, a scan not yet read, the settlement in review |
| 02 | The + menu: Paste, Files, Photos, Scan (the camera furthest from the button) |
| 03 | Paste: text, title and case in one sheet |
| 04 | Review: 30 proposals, markers dimmed |
| 05 | A candidate's sheet |
| 06 | Identities |
| 07 | Replaced: alias chips and the hand-off bar |
| 08 | Paste AI reply |
| 09 | The restored reply |
| 10 | A scan not yet read ("Import not finished") |
| 11 | Its original page (beside the text on the tablet) |
| 12 | Settings |
| 13 | Restore with mdoc |
| 30, 31 | Review and Settings at the largest font (light) |
| 32, 33 | The candidate sheet and the hand-off bar at the largest font (after set only) |

## What the survey found

Fixed before these screenshots:

- The + button had no label for TalkBack: its text was not merged into the
  button. It now carries "Add".
- On the tablet, opening a document with an original crashed the app: the
  side-by-side pane was first measured at zero width and a zero-width page
  bitmap threw. Pages now wait for a width.
- On the tablet, a document not yet read filled the whole row and left the
  original no room.

Open:

- Wide Markdown tables wrap line by line on a phone, in the review text and
  the reply (as on iPhone; see the roadmap's note on wide tables).
- The reply in the fixture uses the desktop session's alias numbers, so one
  value restores to a different original here; the survey's seeding numbers
  aliases in its own order. Not an app defect.

## What changed in the after set

The polish round, against [ADR 0087](../../../adr/0087-ios-ui-polish.md)'s
rules and the before set:

- The document's bottom bar and Restore with mdoc's buttons sat on the
  gesture bar; they now clear it (07, 09, 13, 30).
- Button bars stack at the largest font instead of wrapping (32, 33), and
  TalkBack hears Paste reply, Share reply, Copy reply and Paste another reply.
- Share as Markdown file left the bar for the document's menu, so the bar
  is one row (07).
- Home rows show their status as a symbol and "N to review" reads
  strongest; the Add menu has icons; an empty Home has one (01, 02).
- Counts are singular at one: "1 mention", "1 value restored" (06, 09).
- The document title ends in an ellipsis at large text (30, 33).
- The candidate sheet's alias uses the dark palette in dark mode (05), and
  the Identities sheet shows each category's colour without banded rows (06).
- Restore with mdoc's case reads as a picker (13).

Not visible here: restored replies copied to the clipboard are marked
sensitive (lawkitt/mdoc#321), and an import interrupted off screen says
"The import stopped when mdoc left the screen.", as on iOS.
