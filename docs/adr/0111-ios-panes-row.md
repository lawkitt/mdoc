# The panes' row on iOS

Status: Accepted, 2026-11-11. Amends
[ADR 0110](0110-mobile-editor-polish.md) (the bar while editing),
[ADR 0087](0087-ios-ui-polish.md) (the hand-off bar follows the pane) and
[ADR 0098](0098-mobile-review-bar.md) (where the review bar shows).

## Problem

A document with more than one pane showed a segmented switch in the
navigation bar's centre, in place of the title. On an iPhone, once a
document had a reply the switch read Text, Original, Reply, about 235 pt
wide, in a bar that also holds ☰ and three trailing buttons and leaves
about 170 pt: the switch slid under ☰ and the pencil. The title's menu
moved into ⋯ for those documents, so the same command sat in two places
depending on the document.

## Decision

- **The panes have a row of their own** under the navigation bar, shown
  only when the document has more than one pane. On iPhone it takes the
  content's width, its segments equal: Text, Original, Reply.
- **The bar's centre is always the title's menu**, as with one pane: the
  title truncating at its tail, the chevron, the same menu. ⋯ no longer
  repeats that menu.
- **The row is pinned.** The text scrolls under it and never moves it:
  controls that move under the finger are the class of defect this
  removes.
- **The row shows in edit mode too.** Checking the text against the
  Original is the main reason to edit recognized text. Original still
  hides the keyboard and keeps edit mode on (ADR 0110).
- **iPad and wide windows use the same row**, centred at its natural
  width. It picks the right column there: Original, Reply, and Identities
  in a window narrower than 1100 pt (ADR 0087). A wide window whose right
  column has one choice has no row.
- **Reply stays a pane.** Its header keeps the summary ("N values
  restored") and Remove.
- **On iPhone the Reply pane's bottom bar acts on the reply:** Share and
  Copy of the reply with its names restored, and Paste for another reply,
  in place of Review and Replace N and of the review bar, which act on
  text the reply covers. Back on Text they return. Copy confirms with the
  notice "Copied reply".
- **Wide layout:** the text stays in view beside the reply, so Review and
  Replace N stay until every name is decided; then the bar follows the
  pane as before (ADR 0087).

Rejected:

- **Icons in the segments:** narrower, but a new user has to guess what
  each means.
- **A dropdown in the title:** two taps for every switch, and the panes
  are out of sight.
- **Moving a button into ⋯ when there are three segments:** a button that
  changes place by document, and the bar overflows again at a larger text
  size or with longer labels.
- **Reply as a separate screen:** the reply belongs to the document and is
  read against its text.

## Invariants

- The picker keeps the identifier `pane-picker` and its labels.
- The system's segments keep their text size and shrink a label that does
  not fit, so three segments fit a 320 pt window at every text size.

## Consequences

- The title's width is worked out one way for every document.
- A document with an original or a reply gives the row about 44 pt of the
  screen's height.
- Code: `PanesRow`, `DocumentViewModel+Panes` (which panes a window
  offers, and when the bar acts on the reply), `DocumentScreen`,
  `ActionBar`; tests in `PanesRowTests` and `PanesRowUITests`.
