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
lawkitt/mdoc#319; the tablet set is prefixed `tablet-`.

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
