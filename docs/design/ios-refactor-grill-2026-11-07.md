# iOS next steps and refactor grill, 2026-11-07

The maintainer asked for two things after the sidebar run
([record](ios-sidebar-grill-2026-11-06.md)): what comes next, including new
ideas, and what in the iOS app needs refactoring to stay maintainable and
less buggy. There are no users yet, so backward compatibility doesn't
constrain the refactor.

## Where things stood

- The sidebar run's four PRs were open, green on the fast checks and
  mergeable: #659 (review bar), #660 (core rename and move), #664 (sidebar,
  stacked on #659), #667 (Identities drawer, stacked on #664). The full iOS
  run happens in the merge queue.
- The Find focus fix for #669 was on `macbook-air/ios-find-focus`, worked on
  in another session.
- The iOS app at the stack's tip (`f8126119`): about 20,300 lines of Swift,
  8,900 in the app, 6,100 in `Shared/` (app and Restore extension), 5,300 in
  tests; `crates/mdoc-ffi` about 5,600 lines of Rust. Android is about 7,000
  lines of Kotlin on the older screens.

## What the code review found

Three fresh reviewers read the stack's tip: the core boundary, the document
screen, and the app shell. The findings marked *checked* were confirmed in
the code before this record was written.

### Bugs

1. **Undo across documents** (*checked*). `DocumentScreen` hands the
   window's `UndoManager` to each document's model (`DocumentScreen.swift:74`)
   and never removes its actions, so ⌘Z or a shake after switching documents
   undoes a step in the one before.
2. **A scan that found nothing reopens as unscanned** (*checked*).
   `RealCore.swift:482` sets `scanned = !opened.mentions.isEmpty`.
3. **Identities opens by itself on iPad.** On a wide iPad `onAppear` sets
   `showsIdentities`; when the window narrows to compact the same Bool binds
   to the drawer, which slides over the text.
4. **⌘K on iPhone may not focus search.** The sidebar panel is built only
   while open, so the new `SidebarView` never sees the `searchRequest`
   counter change.
5. **⌘K on a narrow iPad** sets `showsSidebar`, which that layout ignores.
6. **The Documents row keeps a stale case filter.**
7. **Move to case › New case…** doesn't trim, refuse a blank name or number a
   duplicate, so two cases can share a name.
8. **Keep instead in the long-press menu moves on**, where the card's button
   stays on the card (ADR 0101); Return and K on a decided card do the same.
9. **Find previews can garble a letter**: `context(of:)` cuts 50 bytes either
   side, splitting a Cyrillic or accented letter.
10. **A lost cancel**: Swift wires `ffi.cancel()` before Rust's `start_job`
    replaces the cancel flag.
11. **A data race**: `RealSession.path` and `caseID` are read without the lock
    while a background job writes them.
12. **Category order** (*checked*): Swift's `Category` puts Date before Case
    number, Identity, Tax and Bank, unlike `mdoc_pii::Category`, so menus
    differ from the core.
13. **File names drift from the core** (*checked*): `MdocStorage.fileName`
    has 3 of the core's look-alike characters and none of its control
    character, leading-dot or 240-byte rules.

### Structure

- **The fake core re-implements the core.** `FakeCore.swift` (1,195 lines)
  has its own alias numbering, alias checks, category menu, Move and Undo,
  undo history, merge hints, restore and Copy for AI note. Every view-model,
  review-bar, identities, sidebar and screen-snapshot test runs on it. It has
  drifted already: fake mention ids never change, real ones change after a
  Replace. It also ships in the app (`Shared/` has no target exceptions) and
  says it doesn't.
- **Rules computed in Swift**: identity state, kept identities, the phase,
  "no pending proposals" before Copy for AI (Rust doesn't enforce it), the
  alias-taken check as two calls, file names, the sample's occurrences.
- **Cost per tap**: each review tap runs on the main actor and makes a full
  snapshot, the change, an encrypted save and another snapshot; each
  snapshot decrypts the case for the reply and re-parses the Markdown. The
  library snapshot opens every document to draw a row, the Restore extension
  too.
- **One error string**: nine Rust error kinds become `.failed(String)`, shown
  as "could not open this file".
- **Hand-written copies of generated types**: about 400 lines mirror UniFFI
  records (Markdown spans, Category, states, Scope).
- **Navigation state** is spread over `AppModel`, `HomeViewModel` and views,
  with side effects in `didSet` and about 13 presenters on the root.
  `showsIdentities` means inspector, column or drawer by layout and has ten
  writers. ADR 0102 says a narrow iPad slides the sidebar over like iPhone;
  the code uses a third mode.
- **`AppModel`** (470 lines) does navigation, commands, file intake and UI
  test seeding, and builds document models during view rendering.
- **Review selection** has six writers, eleven vars and three change runners.
- **The text view** restyles the whole document on every card change and
  drops a reveal when the snapshot changes but the selection doesn't.
- **Leftovers**: Home's empty state copies Start's; search is written twice;
  `ScreenshotUITests` is orphaned; the spike's FFI ships in the bindings;
  UI tests use fixed sleeps; settings live in three stores.

Fine as they are: `ReviewOrder`, `TextOffsets`, the Jobs code, the setup
download flow, the snapshot tests, the three coarse core objects, the table
placement fix, the copy block and Candidates rotor, the synchronized Xcode
folders.

## Round 1: next steps

### Q1. Merge the stack before refactoring

**Decision:** merge #659 → #660 → #664 → #667 now, in that order, rebasing
each stacked PR after the one below squash-merges. The refactor starts from
`main`.

### Q2. The #669 Find focus fix

**Decision:** its own PR against `main`; already in progress in another
session. If the refactor adds a focus model, that PR removes the retry.

### Q3. Checking items 10–12 on the iPhone 17 Pro

**Decision:** a Release dev build installed on the phone after the merges
for the UI check. One TestFlight build after the refactor lands, which also
closes iOS 0.2 item 8 and iOS 0.1 item 1's device rows.

### Q4. What comes after the merges

**Decision:** a refactor run with iOS features frozen until it lands.
Android is blocked on Test Lab quota and the runner, so it doesn't compete;
moving rules into Rust makes its catch-up cheaper.

### Q5. The seven ideas from the sidebar run

**Decision:**
- **After the refactor:** peek at the originals (idea 2) and Paste a reply
  from Start (idea 3).
- **Then:** a case's page (idea 4) together with Delete a case.
- **Later:** long jobs in the Dynamic Island (1) and a widget (7), which need
  a widget extension and follow Q6; Shortcuts (5, already a To-grill item)
  and thumbnails (6), whose cache must stay out of backups.

### Q6. Nothing outside the app shows a title

**Decision:** an ADR 0085 invariant. Live Activities, widgets, Spotlight
entries, notifications and quick actions show counts and stages ("Recognizing
page 3 of 12", "3 documents to review"), never a document's or case's title,
since titles often hold client names.

### Q7. New ideas

**Decision:**
- **(a) Kept names before sharing** ("Sharing with 2 names kept as they
  are: …" above Copy for AI): a ROADMAP To-grill item, not scheduled.
- **(b) Reply ready on return**: when the clipboard holds text with the
  case's aliases, Start offers to restore it through the system paste
  button, so there is no paste prompt. It is how idea 3 is built.
- **(c) One peek control**: "As the AI sees it" (ROADMAP To grill) folds
  into idea 2, one press-and-hold control instead of two views.

## Round 2: the refactor

### Q8. Bugs first

**Decision:** bugs 1–9, 12 and 13 in one PR, each with a test that fails
without the fix. Bugs 10 and 11 go with Q12.

### Q9. Tests on the real core

**Decision:** the view-model, review-bar, identities, sidebar and snapshot
tests run on the real core over temporary storage, as `RealCoreTests` does.
FakeLibrary, FakeSession and FakeCoreTests go; a small fake Setup and a slow
job wrapper stay for timing tests. Previews use the real core and the sample
document. Unit tests get slower (seconds to about a minute), accepted.

### Q10. Rust owns every rule

**Decision:** [ADR 0104](../adr/0104-mobile-app-structure.md). The snapshot
gains `scanned`, each identity's state, the kept list and stable mention
ids; Copy for AI refuses pending proposals in Rust; errors are typed; the
core exports the file-name rule, finds the sample's occurrences and lists
document summaries without opening each document.

### Q11. Generated types used directly

**Decision:** after Q9, the Swift copies of UniFFI records go; labels stay as
extensions, the app's own ids stay.

### Q12. Threading

**Decision:** core calls stay synchronous on the main actor but get cheap:
one call per tap returning the new snapshot, no second snapshot, no Markdown
re-parse when the text is the same. `RealSession` keeps no state beside
Rust's (fixes the race), and cancellation reaches the job itself (fixes the
lost cancel). Then a tap is timed on the iPhone 12 Pro; async calls on a
background actor only if a tap takes more than about 16 ms.

### Q13. Narrow iPad

**Decision:** ADR 0102 is right: below the regular width, the iPhone's
drawer. The split view's detail-only mode and its ☰ go.

### Q14. One navigation model

**Decision:** one model holds what is open (Start, a section, a document),
whether the sidebar shows, and one presentation value shown from one place.
The user's Identities intent lives in the document model; inspector, column
or drawer are derived from the window size in one place.

### Q15. Document screen

**Decision**, in this order: review selection as a pure state type with one
way to change it and one change runner; the text view rebuilds on a content
change and recolours in place on a selection change, holding a reveal until
layout is done; the long-press menu's actions come from the view model;
`DocumentScreen` split into files.

### Q16. AppModel split

**Decision:** `LibraryModel` (today's `HomeViewModel`), an `Importer`, a
`DocumentStore` outside observation that evicts, a `LaunchConfiguration`
parsed once, and one `Preferences` over one store.

### Q17. Delete

**Decision:** an Undo toast; the delete happens when the toast ends or the
app leaves the foreground.

### Q18. Dead code and leftovers

**Decision:** the spike behind a cargo feature; `addReplacement` and the
orphaned `ScreenshotUITests` go (its OCR resume test keeps a file); Home's
copy of Start and the second search go; Home becomes Documents; the fakes
leave the shipping targets; UI tests wait for elements instead of sleeping.

### Q19. Rules that keep it from growing back

**Decision**, in `docs/development.md`: no Swift copy of a core rule; no
`Task.sleep` or `asyncAfter` in app code unless the line names the iOS bug
and its issue, checked by `just lint`; every bug fix with a test that fails
without it; a fresh reviewer agent on every iOS PR, Codex or not. About 400
lines per Swift file as a guideline, not enforced.

## Round 3: how it runs

### Q20. PRs and order

**Decision:** one PR at a time on the Mac, in this order (ROADMAP iOS items
13–19):

1. Bugs (Q8)
2. Tests on the real core (Q9)
3. The core owns its rules (Q10, Q12)
4. Generated types (Q11)
5. App shell (Q13, Q14, Q16–Q18)
6. Document screen (Q15)
7. Rules and lint (Q19)

B, E and F rewrite the same tests, and two iOS builds with Simulators on the
MacBook Air stalled local CI before. Only the Rust half of item 15 runs
beside them, on ubuntu (`cargo test` on `crates/mdoc-ffi`), and is wired into
Swift on the Mac.

### Q21. Hub, reviews and merges

**Decision:** this session is the hub. Codex is unavailable for two to three
days, so every PR is reviewed by a fresh Opus 5.5 agent. The hub enqueues
each PR once CI is green and the review is clean; the maintainer checks the
result on the TestFlight build. The stack (Q1) merges first.

### Q22. Structural PRs change nothing

**Decision:** items 14, 16, 17 and 18 show a `just ios-survey` run on the
iPhone 17 Pro before and after: an empty diff, or each changed shot with its
reason. Snapshot tests pass with their reference images unchanged.

### Q23. Roadmap shape

**Decision:** one iOS section and one Android section for every version, no
section per version, so the roadmap doesn't grow a section with each
release. An item names its version when that matters. The refactor is items
13–19. Peek at the originals, Paste a reply from Start and a case's page are
items 20–22, each to be grilled before it's built. Kept names before
sharing, the Dynamic Island, a widget and thumbnails go to Later as To-grill
items.

### Q24. The next TestFlight build

**Decision:** `ios-v0.2.0-rc.1`, after the refactor. 0.1 is not released on
its own; its remaining acceptance rows run on this build.

### Q25. Android

**Decision:** Android follows ADR 0104 when it next moves; no Android work
in this run.
