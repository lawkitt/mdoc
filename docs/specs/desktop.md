# Desktop

Folds: 0003, 0011, 0012, 0015, 0019, 0020, 0023, 0024, 0025, 0027, 0028,
0030, 0031, 0032, 0033, 0035, 0038, 0042, 0044, 0046, 0048, 0054, 0055,
0058, 0060, 0061, 0062, 0063, 0064, 0067, 0071, 0073, 0074, 0075, 0076,
0079, 0081, 0084, 0117.

Rules of the macOS and Windows app; headless renders and flow tests specify
the screens. Terms are in [CONTEXT.md](../../CONTEXT.md).

## Documents and session

- Documents are ordinary UTF-8 Markdown files; mdoc is never a notebook
  database or a draft store.
- Each tab keeps one live editor and source preview; switching tabs keeps
  text, undo, selection, search and reading position, and only the active
  view renders.
- Open takes files, not folders, appends them in picker order and activates
  the first; an already open path is reused without replacing its edits.
- Admission runs off the UI thread; a failed tab keeps Retry and Close.
- A PDF or DOCX that needs no OCR converts on first activation; a PDF that
  needs OCR waits for the inline choice even when the runtime is ready.
- One Markdown conversion runs at a time across the app; DOCX preview workers
  are serialized separately.
- Documents other than PDFs convert in a supervised worker, like the DOCX
  preview: a crash, two-minute timeout or cancel kills only the worker, and
  the tab shows an import error.
- A conversion's result stays in its tab and never steals focus; closing the
  tab rejects it, and stale callbacks are rejected by tab identity and
  generation.
- Conversion creates unsaved Markdown and keeps the original; untouched
  automatic output closes without a prompt.
- A conversion never overwrites later edits by running again.
- Saving is an atomic replace with external-change detection.
- Save As never overwrites an imported source or another open tab's file.
- Cancelling a save cancels the close or quit waiting on it.
- Opening and rendering never normalize source Markdown.
- Local images and links resolve from the Markdown's folder, or from the
  original's folder before converted content is saved.
- The session restores lazily from disk and clamps positions; a missing file
  stays listed with Retry and Close, and corrupt metadata shows a notice.
- Session writes are coalesced, serialized, atomic and off the UI thread.
- Launch shows Home or the restored session, never an empty Untitled tab.
- Only an untouched automatic placeholder is disposable; New, or any edit,
  protects a blank tab for good.
- Files dropped on Home, an empty tab or the sidebar open through the one
  open path; a sidebar drop inserts at its line and never replaces a
  document.
- A dropped folder shows the folder sheet before anything opens: its
  supported top-level files, subfolders skipped.
- The sheet ticks only the first 20 new files by name and refuses more;
  already open files stay ticked and don't count. Loose files are not capped.
- The folder sheet's Open files never reads the case key; Open as case and
  Add to "X" read it only after the click.
- If the case key can't be had, the folder's files open without a case and a
  notice says so.
- The DOCX preview's approximation shows as an ⓘ, never a notice; its
  comments and hidden text follow [originals](originals.md).
- In the dark theme PDF and DOCX pages are recoloured to the theme
  (`PageTone`), and the Source header toggle restores original paper.

## Editor

- The editor writes Markdown that renders as it shows; saved bytes and Copy
  for AI are the text as typed.
- Enter in prose and headings inserts a blank line; in lists, quotes, code,
  math and tables it keeps that context's own rule.
- ⇧Enter is a CommonMark hard break (`\` then newline), hidden away from the
  caret.
- ⌫ or ⌦ at a paragraph break removes it in one press, as its own undo step.
- Read-only panes ignore Enter and ⇧Enter.
- The editor renders standard Markdown only: wiki links, `#tags`,
  `key:: value` properties, block references and embeds show as plain text.
- `$…$` math is ordinary text the caret enters; inline image clicks behave
  as text clicks; there are no PDF highlight tools.
- `<br>` renders as a line break only inside a table cell; it is
  display-only, like every rendering rule.
- Display rules never change saved bytes, Copy Markdown or search.
- Tables wrap cells to fit, respecting saved widths, and scroll sideways
  below their minimum; text is never shrunk to fit.
- Keyboard caret and selection changes keep the caret in view with three
  lines of context; mouse clicks and host jumps don't.
- There is no scroll on a keystroke while the caret is within the fitted
  range.
- A held mouse selection autoscrolls near the viewport edge and stops on
  release, even outside the editor.
- On macOS ⌘←/→ go to the visual line's ends, ⌘↑/↓ to the document's,
  ⌥⌫/⌥⌦ delete a word and ⌘⌫ to the visual line's start; ⇧ extends.
- The keymap is a function of the platform, so tests on any OS check both
  key sets.
- In the mention popup ⌘-arrows step mentions; the editor's moves apply only
  with the editor focused.
- Source edits, history, clipboard actions and IME input live in the
  editor's private `input` module; `EditorState` is the sole owner of source,
  selection and history.
- Find searches visible text; Replace inserts the typed text literally, with
  no regex or case preservation.
- Replace never edits hidden Markdown syntax: a match that crosses
  formatting is skipped and counted.
- Replace never edits text inside or overlapping an applied alias.
- One undo reverts a whole Replace all, which searches again first if the
  document changed; Undo and Redo in the find fields act on the document.
- Whole word treats Unicode letters, digits and `_` as word characters, and
  a typographic apostrophe inside a word as part of it.
- mdoc's find index is its own; never copy Zed (GPL) source into the MIT
  crates.
- Spelling never runs on the UI thread, never changes text unless the user
  picks a suggestion, and gives the same result on every platform.
- Spelling skips code, link targets, raw HTML, aliases and PII spans, but
  still flags mixed-script words inside PII spans.

## Shell

- A unified title row holds the traffic lights (drawn caption buttons on
  Windows), the sidebar toggle, Back/Forward and the document header; the
  content is an inset card.
- The sidebar defaults to expanded, is 240 pt until dragged (160–320, saved
  on release), and collapses to a 44 pt rail below a 900 pt window without
  changing the saved choice.
- Hover on the rail reveals the full list after 100 ms with a 120 ms slide,
  over the content without reflow; it closes 300 ms after the pointer
  leaves, and re-entry cancels the close.
- The reveal stays open during a context menu or drag, opens at once on
  keyboard focus, and never changes the saved choice.
- During a sidebar slide only, the document view renders through
  `.cached()`; each slide ends with an uncached repaint, and reduced motion
  never caches.
- The reveal and its close show no hitch over one frame in a native release
  build with a real document and preview open.
- The document header holds one primary action: Pseudonymize, then Apply N,
  then a quiet Share; there is no stepper.
- Every header and ⋯ action is also in the palette and the menu bar.
- 640×480 is a supported window size.
- Back/Forward stops are tab activations, Home and each case page, per
  window, about 50, never restored at launch.
- Back and Forward never offer the page shown, and tabs a close or quit
  prompt brings forward are not stops.
- Zoom scales the Markdown column's text, 50–200% in 5% steps, per
  document, never saved.
- Nothing rests over the text: floating controls hide or move clear of it.
- Notices float as toasts counted by the bell; closing the popover clears
  neither.
- Expected limitations are not notices.
- macOS menus follow the standard layout, in `src/menus.rs`.
- Edit-menu items use the OS actions, so they reach native panels; in a
  text field they change only that field.
- Every user-facing binding is a row in `src/shortcuts.rs`; menus, the
  palette, tooltips and the shortcuts sheet read their keys from it.
- A tooltip names its key from the table.

## Review

- Ticking never changes document text; only Apply, unticking an applied
  entity and undo do.
- Unticking an applied entity undoes it and records Keep; ticking a kept
  mention proposes it again, alone.
- Apply N (⌘↩) applies every proposal as one undo step and closes the popup.
- A click in the text always places the caret.
- Proposed mentions get a light category tint and underline, applied ones a
  blue chip, kept ones a dotted muted underline.
- Category colour never encodes state; fill and chip do.
- Blue means only "applied".
- The responsibility line "Review for missed identifiers before sharing."
  stays beside Apply; no surface claims the text is safe to share.
- The Replacements panel lists category groups in the category menu's fixed
  order, empty groups hidden; category headers carry no state and take no
  input.
- Apply, Keep and Undo leave the selection on the mention acted on; after a
  Keep it moves to the group header.
- Enter outside the alias field applies; inside it Enter only confirms the
  draft.
- The panel and the popup act through the same mapping actions; neither
  adds undo semantics of its own.
- One mention popup follows the active mention through selection, stepping
  and scrolling, without dismissal or a stale frame; offscreen it hides and
  keeps its state.
- Opening, stepping, scrolling or keeping never closes the popup; only Esc,
  ×, a click outside, or applying the last proposal does.
- The popup's scope is This one | All N, All by default; no key changes the
  segment except opening another entity, which resets it to All.
- Enter in the popup applies in the selected scope and moves to the next
  proposal; on an applied or kept mention it does nothing; on a fresh manual
  addition it applies and stays, so Cancel stays offered.
- While a manual addition and an immediate Apply are the latest steps, one
  Cancel undoes both.
- The popup leaves the mention's line and the line below visible, and flips
  above under the same rule.
- Typing an alias is a draft; choosing or confirming it records the
  mapping.
- Escape dismisses suggestions, then the draft, then the popup; switching
  items discards unconfirmed typing without a dialog.
- A hidden-source occurrence shows its containing passage and the exact
  value, never a rendered word as the occurrence.
- Replace (pill beside a settled selection, right-click item, ⌘⌥P) is
  offered once a tab has a review and adds every exact repeat as one
  variant in one undo step.
- A repeat inside a longer mention stays part of it.
- A selection is trimmed of edge whitespace, punctuation and Markdown
  delimiters; one that is still invalid hides the pill, and the menu item
  gives the reason.
- Drag and drop: mention → group assigns, group → group merges, mention →
  empty space splits; each is one undo step with no confirmation.
- Edit, Original, Pseudonymized and Side by side show while replacements are
  applied, and Reply while a reply exists.
- No view ever differs from the editor's text and applied replacements;
  views are read-only and rebuild on every mapping change.
- Opening a comparison view hides the Source preview and restores it on
  close; ⌘F closes the view and searches the editor.
- Decision history lists pseudonymization decisions only, one entry per undo
  point; travelling to an entry equals pressing undo or redo that many
  times.
- Decision history lives as long as the tab and is never written to disk or
  the case, because entries name originals.

## Settings and updates

- Settings is a modal overlay with categories (General, Detection,
  Recognition, Spelling, Cases, About); a click outside closes it.
- Every setting saves on change, atomically; numbers save on Enter or blur, and an
  invalid value keeps the last good one with an inline error.
- A failed save restores the previous value and says so.
- Each category's Reset resets only that category.
- A switch is the only control on its row; keyboard focus only adds the
  ring, so a focused choice never reads as selected.
- An action that can't run is left out, never dimmed.
- On macOS, Light or Dark also sets the app's `NSAppearance`; System clears
  it.
- Automatic update checks are on by default; Include pre-releases is off.
- The update check runs once at launch and on Check now, never in the
  background or on the UI thread, and never blocks startup.
- Update check failures are silent; offline is "no update".
- A pre-release build always considers pre-releases; a stable build only
  with Include pre-releases on.
- On macOS nothing downloads without Download and install; Windows offers
  View release.
- The update card and the "Available" state come only from automatic
  checks; × hides "Available" for that version across launches.
- The dismissed version and the last swap's outcome live in
  `update.json`, never in Settings.
- A newer release discards a staged update and needs a new click.

## Closing and quitting

- A tab is at risk with unsaved changes, an unsaved mapping, or both; the
  mapping counts only while "Ask before closing an unsaved mapping" is on.
- Each at-risk tab asks once; a tab with both offers Save as Case, Save,
  Don't Save and Cancel.
- Quit with two or more at-risk tabs shows a summary naming up to five,
  with Review…, Discard All and Cancel.
- Cancel at any prompt cancels the close or quit, as does Not now in the
  Keychain explainer.
- A failed save of an existing case still offers Retry, even after Discard
  All.
- Save as Case on a never-saved document opens Save As first; cancelling it
  cancels the close.
- Close others asks each at-risk tab in sidebar order; Cancel keeps that tab
  and every one after it.
- Quit resolves dirty tabs before teardown and waits for the final session
  and case saves.

## Menu bar shelf

- With "Show mdoc in the menu bar" on (the default on macOS), closing the
  last window hides it and keeps tabs, undo, mappings and unsaved text; ⌘Q
  or the shelf's Quit really quits. With it off, the last window quits.
- The first time the last window closes with it on, a one-time note says mdoc
  is still in the menu bar and offers Quit when windows close (turns it off).
- mdoc never restarts unasked; Restart to update goes through the ordinary
  quit and its prompts.
- The shelf lists at most 8 documents: open tabs with applied aliases, then
  recently saved case members.
- A document is ready with at least one applied alias and no pending
  proposal; only a ready document copies or drags.
- A closed member is ready only while its file matches the fingerprint saved
  in its case at review.
- A closed member whose file contains a known original is never ready.
- Nothing leaves through the shelf that is not ready.
- The shelf shows and exports only pseudonymized text; originals appear only
  in the clipboard after Restore.
- Previews never show originals or source pages, because the menu bar shows
  in screen shares.
- A drag carries plain text and a fresh `.md` export, named with originals
  pseudonymized, in a private temporary folder cleared at quit.
- The shelf reads a member's file only while it is open and writes no
  document text to app data.
- The shelf never raises a system dialog; a locked case's actions open the
  Keychain explainer in a window.
- Native patches for the shelf live in `vendor/gpui-pre-macos`, each listed
  in its `MDOC-PATCH.md`; the app crate takes no Objective-C dependency.

## Focus, dialogs and theme

- Every control is reachable and activated by keyboard alone on both
  platforms, without a screen reader or Full Keyboard Access.
- F6 / ⇧F6 cycle the shown regions (sidebar, toolbar, editor, replacements
  panel, source preview, comments) and land on the region's last focused
  control.
- F6 into the sidebar always lands on the active tab's row.
- Tab moves through controls in window order across regions; in the editor
  Tab indents and F6 leaves.
- The popup, menus and dialogs keep Tab inside them; modal surfaces make F6
  do nothing, stop window shortcuts and clicks behind them, and return focus
  on close.
- A menu opened by keyboard takes focus, and Esc returns it to the opener.
- One focus ring (`ui::focus_ring`): a 2 px accent ring outside the control,
  never a fill.
- Every dialog is mdoc's own confirm dialog, never a platform prompt, so no
  dialog is titled "Warning" or "Information".
- Buttons follow the platform's order: on macOS destructive choices left,
  default right; on Windows default first, Cancel last.
- The default button is filled, and a scrim dims the window behind every
  modal dialog.
- Files opened from Finder or Explorer while a dialog is up open once it is
  answered.
- Text reaches 4.5:1 and essential non-text 3:1 on every ground it is drawn
  on, in both themes; `style::contrast_tests` checks each pair.
- UI text sizes come from the ramp 10, 11, 12, 13, 14, 15, 16, 18 and 26 px;
  a test scans every `text_size(px(…))`.
- A new colour or size joins the test tables or the ramp in the same change.
- App-owned loading states use one spinner with a label shown at once,
  animation after about 150 ms, and real progress bars only where progress
  is measurable.
- Animation honours Reduce Motion and refreshes at most 30 fps.
