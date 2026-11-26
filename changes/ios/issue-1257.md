- **A restored reply's selection offers Select All again.** Its menu
  showed Copy alone, since UIKit refuses Select All in a text view that
  can't be edited; it now shows Copy, then Select All. The review text's
  selection tests find Select All on iOS 27 too, where the menu's next
  page is behind Next Page, not Forward (fixes
  [#1257](https://github.com/lawkitt/mdoc/issues/1257)).
