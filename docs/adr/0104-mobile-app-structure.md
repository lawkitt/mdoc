# Mobile app structure: the core decides, the app displays

Status: Accepted, 2026-11-07. Sharpens [ADR 0085](0085-mobile-apps.md)
(State across the bridge). Written for iOS first; Android follows the same
rules. Record: [refactor grill](../design/ios-refactor-grill-2026-11-07.md).

## Problem

ADR 0085 says Rust owns document, review, case and restore state and the
platforms hold view state only. After two weeks of fast iOS work the app
breaks that in three ways:

- **Rules in Swift.** Identity state, kept identities, the phase, the
  pending check before Copy for AI, file names and the sample's occurrences
  are computed in Swift. Two of them are already wrong (a scan that found
  nothing reads as unscanned; file names miss most of the core's rules).
- **A second core for tests.** `FakeCore` re-implements alias numbering,
  Move and Undo, undo history, merge hints and restore, and every
  view-model test runs on it, so tests pass on behaviour the core doesn't
  have.
- **State with many writers.** What is open, whether the sidebar or
  Identities shows, and which name the review bar is on are each written
  from many places, and most of the app's recent bugs are two writers
  disagreeing.

## Decision

### The core decides

- Every rule about documents, review, cases, aliases, file names and
  restore lives in `crates/mdoc-ffi` or below. Swift and Kotlin may format
  and lay out what a snapshot says; they never derive a review fact from
  other facts.
- The review snapshot carries what the apps need to display without
  deriving: whether the document was scanned, each identity's state, the
  kept identities, and mention ids that stay the same across a change.
- The core enforces its own refusals (Copy for AI with proposals pending)
  and reports typed errors (refused, alias taken with the identity, not
  found, storage), each shown with its own message.
- The core lists document summaries without opening each document, for the
  sidebar and the Restore extension.
- The apps use the UniFFI-generated types directly, adding labels and icons
  as extensions; they define only their own ids and view types.

### Tests run the real core

- View-model and snapshot tests run the real core over temporary storage
  with fixed case keys, as the core tests do. There is no fake library or
  document session. A fake model setup and a slow-job wrapper may exist for
  timing tests. Previews use the real core and the sample document.
- Test fakes and fixtures are not compiled into the shipping app or the
  Restore extension.

### One owner per piece of state

- One navigation model holds what is open (Start, a section, a document),
  whether the sidebar shows, and the one thing presented over it; one place
  in the view tree presents it.
- A document's model holds the user's intent (Identities wanted, the name
  under review); how it shows (inspector, column, drawer, bar height) is
  derived from the window size in one place.
- Review selection is a value type with one way to change it, tested
  without a view.
- Each document has its own undo history; switching documents never undoes
  another document's step.

### Calls stay synchronous while they are cheap

- Core calls run synchronously on the main actor. A review action is one
  core call returning the new snapshot; the app neither snapshots twice nor
  re-parses Markdown whose text didn't change.
- The platform wrapper keeps no state beside the core's handle, and
  cancellation reaches the running job.
- If a review tap on the iPhone 12 Pro takes more than about 16 ms, core
  calls move to a background actor; until then they don't.

### Timing

- No fixed delays in app code (`Task.sleep`, `asyncAfter`) except to work
  around a named platform bug, or in place of an event that is missing
  until its issue is fixed, with the issue on the line or the comment just
  above. A deliberate on-screen duration (how long a toast stays, a hint's
  beats) is not a wait for an event; it names the ADR that defines the
  behaviour instead. `just lint` checks it. UI tests wait for elements, not
  for time.

## Invariants

- No platform computes a rule the core has; a rule found in Swift or Kotlin
  is moved into the core, not fixed in place.
- Every bug fix has a test that fails without it.
- A fresh reviewer agent reviews every iOS PR, whether or not Codex is
  available.

## Consequences

- About 1,000 lines of fake core and its tests go, and about 400 lines of
  type mapping. Unit tests take longer (about a minute instead of seconds).
- Android gets the review rules from the core instead of writing them a
  third time.
- `mdoc-ffi`'s snapshot and errors change shape; desktop doesn't use them
  and is unaffected.
- Swift files over about 400 lines are a sign to split, not a check.
