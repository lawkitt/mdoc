# Mobile

Rules for the iOS and Android apps. A rule or section marked iOS is iOS
only; Android follows the rest as it reaches each screen. UI and snapshot
tests specify each screen's layout and motion; iOS implementation rules are
in `apps/ios/README.md`.

## Changing the UI

- A UI change starts from an issue labelled `ui`: what's wrong and what good
  looks like. Screenshots show the [sample document](#sample-document) only,
  never a real document.
- Change the tokens or the components first; a screen only composes them.
- Before and after shots (`just ios-survey` or `just android-survey`, into a
  folder outside the repository) go in the PR description.
- The maintainer checks the TestFlight build.
- A grill is only for a pattern this page lacks, which this page then gains.

## The core and the app

- Every rule about documents, Projects, review, aliases, file names and restore
  lives in `crates/mdoc-ffi` or below: the app lays out what a snapshot says,
  never derives a review fact itself, and moves a rule it finds into the core.
- View-model and snapshot tests run the real core over temporary storage
  with a fixed Vault key; there is no fake library or document session.
- Test fakes and the fixed Vault key are never compiled into the app or the
  extension.
- Each piece of state has one owner: one navigation model for what is open
  and presented, and a document's model for intent.
- A review tap saves off the main thread once taps pause, because a save on
  the tap took longer than a frame; a crash before then loses those taps.
- Every bug fix has a test that fails without it.
- A fresh reviewer agent reviews every iOS PR, whether or not Codex is
  available.

## Device and models

- No rules-only tier, because a scan that finds emails but no names gives a
  false sense of safety.
- Below the detector floor the app still imports, recognizes text and marks
  names by selection, and says why where Find names would be.
- The floor is set low, because owners of older phones would rather wait.
- An offered model peaks under 2 GiB and cold-loads within 10 s on each
  device class; an INT8 export also matches FP16 on the qualification
  fixtures.
- A scan needs free memory to start, because the system would end the app
  partway.
- A download on cellular asks first on any cellular interface, whatever the
  system's expensive or metered flag says.
- Nothing starts in the background on its own; only a job the user started
  continues there.
- Nothing outside the app (widgets, Live Activities, Spotlight,
  notifications, quick actions) shows a document's or Project's title, because
  titles hold client names.

## Design

- A Project's color marks the Project only, never a state.
- Anything tappable that isn't a button is a filled capsule; plain text
  never acts.
- A state mark is never an ink disc, because a column of them reads as
  buttons in dark mode.
- A state mark's glyph is the text color, because category colors fail
  contrast as glyphs in light mode.
- At most one primary button per view; a drawer or an iPad column counts as
  its own view.
- No text is hyphenated anywhere.

## Rendered Markdown

- The text view's string is the Markdown source, so candidate offsets are
  the core's UTF-8 source offsets and Copy for AI bytes match desktop.
- iOS draws tables, lists, quotes, alerts, code and rules in one TextKit 2
  view, never a web view.
- Ordered items show the number written in the text, never a count, because
  a clause number is content.
- Test documents copy the structure of real ones, never their names or
  numbers.

## Navigation

- The sidebar mirrors desktop's Vault hierarchy (`Home`, `To review`, inline
  `Projects`, and `Unfiled` / `All Documents`) and shows titles and project
  names only, never a document's text or identities.
- Tapping a Project opens a dedicated Project page titled with the Project's
  name and color (`Documents` · `Replies` · `Names`), never an unlabeled
  filtered Documents list.
- New imports default to **Unfiled** (or the open Project when added from inside
  one); iPhone keeps single-active-document navigation with no tab strip.
- Document text leaves the app only through Copy for AI, Share, or a device
  transfer the user starts ([ADR 0127](../adr/0127-cross-device-p2p-vault-transfer.md)):
  no plain Copy, Share, Look Up, Translate, Search Web or Writing Tools on it,
  in edit mode too, where Copy writes a pasteboard only mdoc reads.
- Copy for AI and Share are always in the title menu; until the document is
  ready they are off, with the reason under the title.
- With more than one pane, a pinned row under the bar switches Text,
  Original and Reply, because controls that move under the finger get
  mis-tapped.

## Review

- Replace and Keep apply on tap; there is no staged Apply.
- The text scrolls only as far as it takes to show the name between the
  bars, because a fixed target position read as random scrolling.
- "All N reviewed · Undo" shows only when this visit of the bar decided or
  changed something, because its Undo would reach an earlier decision.
- mdoc draws one hint at a time itself, never as a popover, because
  popovers hid the bar's controls.
- Not a name sits last in its menus, unstyled, because Undo takes it back.

## Identities

- Replace N is not a primary button, because it is a bulk shortcut.
- No full swipe on a row, because Keep sends the original to the AI and
  one swipe back ran Replace.
- Mention drags and drop-to-split are desktop only.

## Edit mode

iOS only.

- Smart quotes, smart dashes and auto-pairing are off, because saved bytes
  are the text as typed.
- A command edits only what it changes (line prefixes, wrap markers, a
  task's box), and no table command pads or realigns columns, because an
  edit that runs into a proposed name drops it.

## Gestures

- No action exists only as a gesture: each has a visible control. Redo, as
  Undo's long press, is the one exception, as it reverses the visible Undo.
- A drawer swipe never takes a vertical scroll.

## Wording

- Short, plain verbs: Replace, Keep, Same as…, New alias, Undo.
- Toasts name what happened: "12 replaced", "PERSON_3 is now PERSON_1".
- Plain model names ("Name finder").
- English strings are word for word on both platforms, in US spelling.

## Accessibility

- Every screen works at every Dynamic Type size, up to the largest
  accessibility size, with nothing clipped.
- Every icon-only control has a label, and no hit target is under 44 pt.
- Screen-reader support beyond labels waits for the roadmap's
  Accessibility pass; existing VoiceOver and TalkBack code stays but is not
  required in new work.

## Sample document

- `Shared/Core/SampleDocument.swift` is Try a sample and the fixture for
  previews and tests. Its names are made up and it never holds a real
  person's data, so its screenshots are safe to share.
