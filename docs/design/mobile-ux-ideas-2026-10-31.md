# Mobile UX ideas, 2026-10-31

Ideas for the iOS app (and Android where noted) after iOS 0.1's code and
polish landed. These are ideas to grill, not decisions. Sources:

- the after set of the [iOS UI survey](../evidence/ios-app/2026-10-31-ui-survey/README.md);
- the app's code on `main` at d95d0ba;
- a desk scan of mobile redaction apps and of what iOS 26 offers.

The ideas that need a grill are at the end of `## Later` in
[ROADMAP.md](../../ROADMAP.md).

## Rules every idea keeps

From [ADR 0085](../adr/0085-mobile-apps.md),
[0087](../adr/0087-ios-ui-polish.md) and
[0088](../adr/0088-ios-on-real-devices.md):

- **Review first:** nothing is shared that the user has not reviewed.
- **Read-only source text:** candidate offsets are the core's.
- **Same export path as the desktop:** Copy for AI bytes match the
  desktop's.
- **No network** except model downloads.
- **Nothing in backups.**
- **VoiceOver and Dynamic Type** on every control.
- **One model job at a time.**
- **Below the 8 GB floor:** no name finding, but marking still works.

## What the app has today

**Home**
- A flat list of documents, newest first.
- The + menu offers Scan, Photos, Files and Paste.
- Swipe to delete.
- No search.

**Document**
- Markdown source, with candidates underlined.
- Tapping a candidate opens a sheet: Replace or Keep, the alias, all
  occurrences or just this one, and Next proposal.
- An Identities sheet, with Replace all and Keep all as swipes.
- Select text, then Replace… marks missing text.
- On iPhone, Text and Original are a switch; on iPad they sit side by side.

**Hand-off**
- The bar reads Share, Copy, Paste reply.
- Long-pressing Share offers a Markdown file.
- The reply pane shows restored values highlighted.
- The Restore with mdoc share extension offers Copy, Share and Save to
  document.

**Not used yet**
- App Intents (Shortcuts and Siri), app lock (LocalAuthentication).
- A cover for the app switcher.
- Undo of any kind.
- Find in document.
- Drag and drop.
- Hardware-keyboard shortcuts.
- Haptics.
- Tips (TipKit).
- Home Screen quick actions.
- The merge hint and Same as… ([ADR 0080](../adr/0080-merge-hint.md)).

**One finding from the code:** Copy reply and the extension's Copy write
the restored reply, with real names, to `UIPasteboard.general` with no
expiry. Universal Clipboard can then hand it to the user's Mac. The
reply view (`RestoredTextView`) also lacks the review view's Writing Tools
opt-out.

## Mobile field notes

- **[CleanPII](https://apps.apple.com/app/cleanpii/id6760742759)** ($0.99,
  iPhone, on device): text only.
  - Four styles: tags, partial mask, hash or removal.
  - A mapping table that restores in one tap, with up to five saved groups.
  - A regex allowlist and three strictness modes.
  - No share extension described.
- **[Obscure](https://apps.apple.com/app/id1413188754)**: images.
  - Detection on the device, plus one tap for "emails and phones".
  - Pinch to zoom and undo while reviewing; batch mode.
  - **Shortcuts actions** for automation.
  - **Home Screen quick actions:** last photo, camera, copied image.
- **Image redactors** ([Maskr](https://apps.apple.com/app/id6764877544),
  [Mosaic](https://apps.apple.com/us/app/-/id6756632173),
  [Redact Kit](https://mwm.ai/apps/redact-kit-clean-privacy/6769065437)):
  - tap a word to redact it;
  - flatten exports;
  - strip metadata.

  None restores.
- **ChatGPT** has a paste-and-review "redact before you prompt" app inside
  ChatGPT. That review happens inside the AI service.
  - On iOS, ChatGPT's share sheet always starts a new chat; users ask to
    pick an existing one
    ([forum](https://community.openai.com/t/ios-share-sheet-allow-choosing-an-existing-chat-instead-of-always-creating-a-new-one/1389070)).
  - So mdoc's Share lands in a fresh chat each time.
- **iOS 26 Shortcuts** has a "Use Model" action that sends a prompt to
  Apple's on-device model, Private Cloud Compute or ChatGPT
  ([MacStories](https://www.macstories.net/stories/ios-and-ipados-26-the-macstories-review/8),
  [Tom's Guide](https://tomsguide.com/computing/software/apples-shortcuts-app-is-getting-a-huge-upgrade-in-ios-26-and-macos-26-heres-how-it-will-help-you)).
  App Intents for mdoc would let a Shortcut chain Share text → Use Model →
  Restore.
- **`BGContinuedProcessingTask`** already shows a long job's progress as a
  system Live Activity, with title and subtitle the app sets
  ([Apple](https://developer.apple.com/documentation/backgroundtasks/bgcontinuedprocessingtask)).
  A custom Live Activity would add little.
- **None of the mobile rivals found combines review, a case and restore.**
  mdoc's mobile edge is the same as on the desktop.

## Ideas

Grouped by job. **(R)** marks the ones on the ROADMAP.

### Safety on a phone

1. **(R) Restored text stays off the shared clipboard.**
   - Copy reply and the extension's Copy write `localOnly` with an
     expiration (for example ten minutes) through `UIPasteboard.setItems`.
   - Copy for AI can stay shared: it carries aliases only.
   - Android 13+: mark the clip with `ClipDescription.EXTRA_IS_SENSITIVE`,
     so the clipboard preview hides it.
   - `RestoredTextView` (reply pane and extension) opts out of Writing
     Tools as the review view does.
   - The desktop has the same question for its Restore Clipboard.
2. **(R) A cover in the app switcher, and an optional Face ID lock.**
   - When the scene leaves the foreground, draw a plain cover. Otherwise
     the snapshot shows client names.
   - An opt-in "Require Face ID" (passcode fallback) protects the app and
     the Restore extension.
   - Phones are handed over, shown across a table and lost.
   - Android: `FLAG_SECURE` on document screens, and the same opt-in
     biometric lock.
3. **(R) Remove documents after a while.**
   - A Settings choice: Never, 7, 30 or 90 days. Documents and replies go;
     the case stays so later replies still restore.
   - Settings also shows storage: models, documents, cases.
   - Phones gather client files quietly; the desktop's file lifecycle
     (ADR 0003) has no phone equivalent.
4. **Screen recording and mirroring.** When the screen is captured
   (`UIScreen.isCaptured`), show "as the AI sees it" (idea 8) by default.
   This guards demos and screen shares. Grill together with 8.

### Reviewing on a small screen

5. **(R) Review one proposal at a time.**
   - A full-screen card: the proposal in its sentence, its category and
     alias.
   - Replace and Keep at thumb height, or swipe them; "3 of 12"; haptics;
     undo the last one.
   - Today the candidate sheet covers half the text and Next proposal sits
     at the sheet's foot.
   - Desktop's mouse wheel steps the selection (ADR 0033); this is the
     phone's equivalent.
6. **(R) Undo.**
   - Undo Replace, Keep, Replace all, Keep all, Remove reply and Delete
     document.
   - An Undo toast, plus shake and three-finger swipe through
     `UndoManager`.
   - The desktop's merge already has an Undo toast (ADR 0080).
7. **(R) The merge hint and Same as… on mobile.**
   - ADR 0080's "Looks like Maya Bennett · Merge ×" on the Identities row.
   - Same as… in the candidate sheet.
   - Without them, "Ms Bennett", "Maya" and "Mia" reach the AI as three
     people on a phone (survey shot 12).
8. **(R) "As the AI sees it."**
   - A toggle that shows the text with aliases in place: the Copy for AI
     text, read-only, with any original still in it highlighted.
   - Today the user shares without seeing what leaves.
   - The desktop has no such view either; worth grilling for both.
9. **(R) Find in document, then mark every match.**
   - `UIFindInteraction` (⌘F on iPad).
   - Replace all matches as one identity.
   - Finds a surname the detector missed, and repeats that selecting each
     one would miss.
10. **Proposal ticks on the scroll bar.** A thin rail of category-coloured
    ticks for proposals still to review, and tap a tick to jump. Small
    phones scroll a long way between proposals.
11. **(R) Highlights on the original page.**
    - For scans and photos, draw the found names' boxes on the page image.
    - OCR knows where each line is; whether it keeps word boxes is to be
      checked.
    - Checks OCR against the page where it matters most.
12. **Pencil and tap marking.**
    - On iPad, strike through a word with Apple Pencil to mark it.
    - On iPhone, a "Mark" mode where one tap selects a word and a second
      tap extends to the next capitalised word.
    - iOS selection handles are slow for two-word names.

### Getting text in and out

13. **(R) Text from any app becomes a document.**
    - Sharing text, a web page or a Mail selection to mdoc opens it as a
      pasted document in the app.
    - Text goes through an extension that hands it over. Extensions' memory
      limits are far below the detector's 2 GiB, so the app does the work.
    - **Android:** `ACTION_PROCESS_TEXT` puts "mdoc" in every app's
      text-selection menu.
    - Today only files arrive by share; text needs Paste.
14. **(R) Share a selection.** "Share for AI" in the selection menu sends
    one clause through the same export path, with the placeholder note.
    Phone chats are short, and a whole contract is often more than asked.
15. **(R) Shortcuts and Siri (App Intents).**
    - Actions: "Restore text with a case", "Get a document's text for AI"
      (reviewed documents only) and "Add text to mdoc" (opens it for
      review).
    - With "Use Model", a Shortcut can share a document, ask ChatGPT or
      Apple's model, and restore the answer.
    - The grill must keep review first: no intent hides names without
      review.
16. **Reply on return.** When the user comes back from an AI app, a "Paste
    reply" banner appears above the bar, using `PasteButton` so there is no
    paste prompt. Today Paste reply sits third in the bar.

### Learning the app

17. **(R) A demo document before the download.**
    - The empty Home offers "Try a sample". The sample is the settlement
      letter with stored proposals (the parity goldens), so review, share
      and restore all work without the 649.5 MB download.
    - Today the first screen asks for the download before showing what
      mdoc does.
18. **(R) Hints, haptics and quick actions.**
    - TipKit, once each: "Select text to mark a name", "Long-press Share
      for a Markdown file", "Restore with mdoc is in the share sheet".
    - Haptics on Replace, Keep and a finished scan.
    - Home Screen quick actions: Scan, Paste, Paste reply.
    - Long-press and the share extension are invisible until someone tells
      you.

### iPad

19. **(R) iPad as a work surface.**
    - Drag a PDF from Files or Mail onto mdoc to add it.
    - Drag the text for AI out to ChatGPT or Claude in Split View, and drag
      the reply back in to restore.
    - Keyboard: ⌘F, R and K, arrows for the next proposal, ⌘⇧C to copy for
      AI.
    - Two windows for two documents.
    - Stage Manager and a keyboard make the iPad the desktop's closest
      substitute.

### System surfaces

20. **A notification when a long job ends in the background**, such as
    "Settlement agreement: 12 to review". The title only, never a name.
    Asked for at the first long job, as Android's ADR 0089 does.
21. **A case-aware keyboard** (long shot).
    - A custom keyboard that offers a case's aliases for names typed in any
      app. It is a lookup, with no model.
    - Custom keyboards need Full Access to read the App Group, and that
      prompt deters lawyers.
    - Grill only if users ask to type follow-up questions with names.

## Not doing, and why

- **Widgets and Spotlight indexing:** they would put document titles and
  names on the Lock Screen and in system search.
- **A live camera view with names underlined:** PP-OCR and the detector do
  not run per frame within the budgets, and ADR 0085 keeps recognition on
  PP-OCR rather than VisionKit.
- **Apple Intelligence Writing Tools on mdoc's text:** it would hand
  originals to a model mdoc did not choose. The review view already opts
  out (`MarkdownTextView`, `writingToolsBehavior = .none`). The restored
  reply's `RestoredTextView`, in the app and the extension, does not; it
  is folded into idea 1.
- **A custom Live Activity:** `BGContinuedProcessingTask`'s system progress
  covers it. Check that its title and subtitle read well ("Recognizing page
  4 of 20").
