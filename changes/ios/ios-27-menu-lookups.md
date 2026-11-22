- **UI tests pass on iOS 27, and one leaves quarantine.** The Panes row
  and Dark page tests find the title menu's items by their words, as iOS 27
  drops their identifiers, so their absence checks test something there;
  the Dark page test reads the moon's On or Off on iOS 26 only, as iOS 27
  drops a toolbar button's accessibility value.
  `EditModeUITests/testTypeUndoLeaveAndRelaunch` sends ⌘Z again until it
  lands and stops mdoc only once it is in the background, and CI runs it
  again (#818); a quarantine line naming a renamed test goes (#837).
