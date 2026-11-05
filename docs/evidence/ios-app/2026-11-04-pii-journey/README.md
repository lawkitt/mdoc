# Name-replacement journey, 2026-11-04

Before and after shots for the
[journey survey](../../../design/ios-pii-journey-survey-2026-11-04.md).
They were taken on the iPhone 17 Pro Simulator (iOS 26.1) and the iPad Pro
13-inch (M5), downscaled to 1000 px high. Every document shown is the
fictional demo agreement from `docs/product-media`, or a note written from
its names.

## before/

- `NN-…-light|dark.png`: the build 734 UI survey (`SurveyUITests`, which
  scans with the structured rules, so only the email is proposed).
- `ipad-14-…`: the same survey on the iPad: the candidate sheet as a short
  card that cuts off its last rows.
- `ax-…`: `LargeTextExploreUITests` at AX XXXL on build 734.
- `live-…`: by hand with the real name detector:
  - `live-04`: a long paste hides Title and Case.
  - `live-14`: the candidate sheet's first height hides Apply to and Next
    proposal.
  - `live-16`: Replace all with no feedback.

## after/

The same screens from this branch, under the same names.

- The survey (`NN-…`, `ipad-…`) ran with the review text view's
  `accessibilityValue` override (#594) removed locally. With it, the survey
  can't find the .docx email candidate as a link; see the survey note. The
  override changes no pixels.
- `ipad-14` and `ipad-19`: the review sheets at full height on iPad.
- `ax-…`: `LargeTextExploreUITests` at AX XXXL. `ax-03` shows Settings'
  Download; `ax-05` and `ax-06` show the candidate sheet with lists; `ax-08`
  shows the two-row hand-off bar.

The `live-…` shots were taken by hand with the real name detector:

- `live-07-find-names-notice`: the notice after Find names.
- `live-14-candidate-first-height`: the whole candidate sheet at its first
  height.
- `live-14-candidate-revealed`: a name tapped near the bottom, scrolled
  above the sheet.
- `live-14-candidate-at-the-end`: the last name in the document, above the
  sheet.
- `live-14-candidate-dark` and `live-17-identities-dark`: both in dark.
- `live-16-replace-all-notice`: the notice after Replace all.
- `live-04-paste-long`: Title and Case stay on the Paste sheet.
- `live-25-remove-reply`: Remove reply asks first.
