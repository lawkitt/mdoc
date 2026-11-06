# iOS name-replacement journey survey, 2026-11-04

A screen-by-screen walk of the iOS app's name-replacement journey after
build 1 (0.1.0, build 734): first launch, Add document, Find names, review,
Replace…, Identities, Share and Copy for AI, Paste reply and Restore with
mdoc, Settings and About. It ran on the iPhone 17 Pro Simulator (iOS 26.1),
in light and dark, at the default text size and at AX XXXL, with the iPad
Pro 13-inch (M5) where the layout differs.

Before and after screenshots are in
[`docs/evidence/ios-app/2026-11-04-pii-journey`](https://github.com/lawkitt/mdoc/blob/1e7096412fda2d27ec5a274e5b53ba00c3b101f1/docs/evidence/ios-app/2026-11-04-pii-journey/README.md).
The "before" set is the build 734 survey. Find names ran with the real name
detector, cloned into the Simulator from the Mac's desktop install, on the
demo settlement agreement (`docs/product-media`).

The rules every change keeps:

- ADR 0085, 0087 and 0096: read-only text, and candidate offsets stay the
  core's source offsets.
- Every string in the String Catalog.
- VoiceOver labels and actions.
- Dynamic Type, and light and dark.
- Copy for AI bytes unchanged.

## Fixed

### Review

1. **The candidate sheet hid its own controls.** At its first (half)
   height, Apply to (All N occurrences / Just this one) and Next proposal
   were below the fold at the default text size.
   - The category moves into the title, with its colored icon, and the
     separate Category row goes.
   - Apply to is a segmented control.
   - Sections sit closer together.
   - When the sheet has Apply to or Next proposal, its first height is two
     thirds of the screen, so the whole sheet shows without dragging.
2. **Next proposal could open a name hidden under the sheet.** The text
   never scrolled to the candidate under review. Now opening a candidate
   scrolls it into the top third of the text, the part the sheet leaves in
   view.
   - In a table, the grid also scrolls sideways to the name.
   - Near the end of the document, the text gets room below its end while
     the sheet is up.
   - The scroll is not animated with Reduce Motion on.
3. **iPad: the candidate sheet was a short card** that cut off its
   explanation and Apply to (medium detent on a form sheet). On iPad the
   review sheets now open at full height.
4. **AX XXXL: the review sheets opened at half height**, showing a line or
   two.
   - At the accessibility sizes the candidate sheet, Replace…, Add document
     and the Identities sheet open at full height.
   - Replace / Keep and Apply to become lists there, since a segmented
     control's text does not grow.
5. **After Find names nothing said where to look.** A notice now says
   "23 to review: tap underlined text". An empty scan already said so
   (ADR 0087).
6. **Replace all changed every proposal silently.** A notice now says
   "21 replaced. Tap one to keep the original." VoiceOver reads the button
   as "Replace all 21 proposals", with a hint that each can still be kept.
7. **A newer notice vanished at once.** The old notice's timer, when
   cancelled, still cleared the notice, so a second notice (Copy, then
   Replace…) never showed. Notices now also stay for 2 to 4 seconds by
   their length, instead of 2 seconds for every sentence.
8. **Find names and Identities had the same icon**
   (`person.text.rectangle`, which is also the Identity category's icon).
   Find names, and Home's "Ready to find names", now use
   `text.magnifyingglass`.

### Identities

9. **"Replaced · 1" did not say what the 1 counts.** The chip now says only
   the state. A plain "4 mentions" line shows under the alias when a name
   appears more than once. VoiceOver already read "4 mentions".

### Adding a document

10. **A long paste pushed Title and Case off the Paste sheet.** The editor
    now scrolls inside a capped height (260 pt), so the case picker stays
    in view.
11. **Two cases could share a name.** Pasting the same document twice gave
    titles "Settlement agreement" and "Settlement agreement 2", but both new
    cases were named "Settlement agreement", which the Restore with mdoc
    case picker cannot tell apart. A new case now takes the same number the
    core gives the document's title. The name is trimmed first, as the
    core trims it.

### Hand-off and reply

12. **AX XXXL: the Share / Copy / Paste bar took about a third of the
    screen** (about 257 pt of 874).
    - The bar's text stops growing at the second accessibility size, as
      the system's own bars do. A long press shows a label in the large
      content viewer.
    - Where three buttons don't fit in a row, the primary one takes a row
      and the others share the next.
    - The bar is now about 150 pt. This changes ADR 0087's "stacks its
      buttons" wording; the ADR is amended to match.
13. **Remove reply deleted the saved reply with one tap**, with no undo and
    a 30 pt target. It now asks first ("Remove the reply?"), and the target
    is 44 pt.

### Settings and About

14. **AX XXXL: Settings' Download button read "Down- / load 18…"** (P13,
    ROADMAP). At the accessibility sizes the size now goes under the word,
    with no icon.
15. **UK spelling.** "unrecognised" is now "unrecognized" (as desktop says)
    and "Licence" is now "License". Android keeps its ROADMAP item.

## Logged, not changed

- **UI tests can't find the .docx email candidate as a link since #594.**
  #594 overrides the review text view's `accessibilityValue` (VoiceOver
  hears the rendered text). Since then, the survey's `test2Review` and the
  quarantined `ReviewFlowUITests` fail on main in the Simulator: no link
  named `maya.bennett@example.com` appears in the imported demo .docx,
  though the Markdown survey still finds its links. Removing the override
  makes `ReviewFlowUITests` pass. Overriding `accessibilityAttributedValue`
  with the link attributes kept did not help. VoiceOver may be affected
  the same way; the Candidates rotor still reaches every candidate. The
  fix needs a VoiceOver check on a phone, so it is a follow-up. The after
  survey here ran with the override removed locally, which changes no
  pixels.

- **Share from mdoc lists mdoc itself.** The share sheet's app row offers
  Restore with mdoc (labelled "mdoc") for mdoc's own text, where it does
  nothing useful. Excluding it by its bundle ID would also remove the only
  path the survey and screenshot tests use to open the extension in the
  Simulator. It needs another route into the extension first.
- **"1 unrecognized" is terse for a first-time reader.** It is desktop's
  wording (`src/pii/ui/reply.rs`), so a change belongs to both apps. That
  is the maintainer's call.
- **"Maya Bennett" and "Bennett" become two identities** (PERSON_1,
  PERSON_2). This is the core's linking, not the UI.
- **Next proposal opens the first proposal left, from the top**, not the
  one after the current candidate. Decided ones drop out, so it still walks
  the document in order. Kept.
- **Home says "Ready to share" once a reply is restored**, with no sign of
  the saved reply. Changing it is a status wording decision; left for the
  maintainer.
- **Leaving and returning during Find names** was not seen here: the scan
  took about 3 seconds in the Simulator. `DeviceLongJobsUITests` and P17
  cover it on the phone.
- **The Simulator asks before every download** (its network is not Wi-Fi).
  Not re-checked: the models were cloned in from the Mac, so nothing
  downloaded.
- **The Identities sheet's Done is plain text**, unlike the glass buttons
  of the other sheets. It is the list's own title row, so the document's
  title stays in the bar on iPhone. Kept.

## For the maintainer's eye on a phone

- The candidate sheet's two-thirds first height and the scroll to the
  candidate, on the 17 Pro and the 12 Pro.
- The hand-off bar at AX XXXL. Check the large content viewer (long press)
  and VoiceOver's names: "Share", "Copy", "Paste reply".
- Whether the notices' 2 to 4 seconds read well.
