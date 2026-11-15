# Mobile app structure: the core decides, the app displays

Status: Accepted, 2026-11-07.

## Problem

After two weeks of fast iOS work the app broke ADR 0085's split of state:
review rules were computed in Swift (two of them wrong), a `FakeCore`
re-implemented the core for every view-model test, so tests passed on
behaviour the core didn't have, and pieces of state had many writers, the
source of most recent bugs.

## Decision

- **The core decides.** Every rule about documents, review, cases, aliases,
  file names and restore lives in `crates/mdoc-ffi` or below. The review
  snapshot carries what the apps display; the core enforces its own refusals
  and reports typed errors. The apps use the UniFFI-generated types
  directly and never derive a review fact from other facts.
- **Tests run the real core** over temporary storage with fixed case keys.
  There is no fake library or document session; only a fake model setup and
  a slow-job wrapper for timing tests. Fakes never ship.
- **One owner per piece of state:** one navigation model for what is open
  and presented; a document's model for the user's intent, with how it shows
  derived from the window size in one place; review selection as a value
  type with one way to change it; an undo history per document.
- **Calls stay synchronous while they are cheap.** Core calls run on the
  main actor; a review action is one call returning the new snapshot. They
  move to a background actor only if a review tap on the iPhone 12 Pro
  passes about 16 ms. A tap's save runs off the main thread once taps pause
  (`DeviceTapTimingUITests` measured 17–19 ms with the save, under 2 ms
  without), accepting that a crash in that window loses the last steps.
- **No fixed delays in app code** except for a named platform bug, a
  missing event or a deliberate on-screen duration, each marked; `just lint`
  checks it (`scripts/fixed-delays.sh`).

## Invariants

- No platform computes a rule the core has; a rule found in Swift or Kotlin
  moves into the core.
- Every bug fix has a test that fails without it.

## Consequences

- About 1,400 lines of fake core and type mapping went; unit tests take
  about a minute.
- Android gets the review rules from the core instead of writing them again.
- `mdoc-ffi`'s snapshot and errors changed shape; desktop doesn't use them.

References: [refactor grill](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/design/ios-refactor-grill-2026-11-07.md); rules in [docs/specs/mobile.md](../specs/mobile.md).
