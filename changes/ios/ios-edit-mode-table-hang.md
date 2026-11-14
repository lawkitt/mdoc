- **Edit no longer freezes a document with a large table, or jumps to its
  end.** Tapping Edit after selecting a word in a table could leave mdoc
  unresponsive until iOS closed it, and entering edit mode partway down a
  long table showed the end of the document. The table switches after the
  screen's update, a selection in a cell ends first, and the line at the
  top of the view stays there
  ([ADR 0114](docs/adr/0114-ios-menu-swipes-and-name-words.md)).
