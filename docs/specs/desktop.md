# Desktop

Rules of the macOS and Windows app that the code can't show; headless
renders and flow tests specify the screens. Terms are in
[CONTEXT.md](../../CONTEXT.md).

## Scope

- Imported documents, Markdown, review state, Source Originals, Projects and AI
  Replies live in mdoc's encrypted Vault ([ADR 0125](../adr/0125-unified-encrypted-vault-and-projects.md))
  and autosave, with no save prompt.
- Opening a `.md` imports a copy like any other format; nothing is edited in
  place on disk, and `Export…` writes standard UTF-8 Markdown.
- Pseudonymize clipboard creates no document.
- The editor renders standard Markdown only: wiki links, `#tags`,
  `key:: value` properties, block references and embeds show as plain text.
  There are no PDF highlight tools.
- mdoc's find index is its own; never copy Zed (GPL) source into the MIT
  crates.
- Spelling gives the same result on every platform.

## Shell

- The left sidebar is a stable Vault tree (Search, Home, Projects, Unfiled) and
  the only document navigation: no tab bar and no open or closed state,
  because every sidebar failure in the 2026-11 audit came from that split.
- Recently viewed documents keep their undo and scroll; `⌃Tab` switches by
  recency.
- Projects start collapsed and remember their expansion; Unfiled shows its 7
  most recent and Show all.
- The Project page uses three segments (`Documents` · `Replies` · `Names`) so
  identity lists never crowd out working documents.
- The document header holds one primary action at a time; there is no
  stepper.
- Every header and ⋯ action is also in the palette and the menu bar.
- Nothing rests over the text: floating controls hide or move clear of it.
- Expected limitations are not notices.
- The collapsed sidebar's reveal and its close show no hitch over one frame
  in a native release build with a real document and preview open.
- Unfiled lists the newest import first and a Project its documents by title,
  so a row never moves while its document is edited.
- Only Unfiled rows offer Delete, and it always asks first; a Project's
  document leaves with Move to Unfiled.
- Dragging a document row onto a Project or Unfiled moves it; files dropped
  on a Project row import into it, anywhere else into Unfiled.
- The 8 most recently viewed documents stay live; a ninth evicts the least
  recent once its autosave is written. Close (`⌘W`) shows Home and keeps the
  window and the view.
- The collapsed rail shows Search, Home and New; the tree is in its reveal.

## Documents

- Open, drop, paste and New each make an Unfiled document, or one in the
  Project they were started from; a New or paste becomes one at its first
  keystroke, and an empty New is dropped when left.
- A repeated import asks once per open, with Open Existing as the default;
  the same bytes twice in one open import once.
- Export Markdown starts in the folder of the last file imported or exported
  in this run, else Documents, never the working directory.
- Move to New Project works for any Vault document. A move clears the review,
  so the moved document scans again, and its toast offers Undo for 10 s.
- An empty Project's delete asks Delete or Cancel; a non-empty one adds Move
  to Unfiled and Delete Documents.
- The Project page opens on `Documents`; Replies list newest first, and the
  Names search is always shown. Segments have no shortcuts.
- Notes are an Inspector tab beside Review and Share, saved when typing
  pauses and before a close or quit.

## Replies

- A reply pasted into the reply box or by Paste AI Reply is saved once it
  settles, titled "Reply to <document title>". An edit in the box replaces it,
  clearing keeps it, and a reopened document's box is empty.
- The Reply page is read-only and rebuilt from the Vault each time it shows,
  so a move, merge or forget shows at once. Replies are not renamed, and
  Delete reply asks first.
- Promote to Document opens the new document, whose header reads "From AI
  reply" until the reply is deleted.

## Home and search

- Home is a launcher: drop zone, New document, Paste text, Paste AI reply,
  Pseudonymize clipboard, and the Continue strip. It lists no Projects or
  recent files.
- The Continue strip shows documents that need review and saved replies,
  newest first, three cards. A document needs review until it is saved
  Ready, or while a live view has proposals; one kept from AI never does.
- The palette searches titles and Project names from the manifest; every
  query word must match. Title prefixes rank first, then word starts, then
  any title match, then a match through the Project's name.

## Recovery and the shelf

- Show Recovery Key reads the key on click only and forgets it on Hide,
  another category or closing Settings.
- One recovery-key nudge follows the first Project; Not Now or showing the
  key ends it.
- The shelf lists open documents with applied aliases first, then stored
  ones, newest first, up to 8. A row is ready only after the outbound check,
  and reads "Checking names" while a write is pending.

## Updates

- The update check runs at launch and on a user action, never periodically,
  and never blocks startup.
- Nothing downloads until the user clicks Download and install.
- mdoc never restarts unasked; Restart to update goes through the ordinary
  quit and its prompts.

## Review

- The responsibility line "Review for missed identifiers before sharing."
  stays beside Apply; no surface claims the text is safe to share.
- Category colour never encodes state, and blue means only "applied".

## Platform

- The app crate takes no Objective-C dependency; the shelf's native patches
  live in `vendor/gpui-pre-macos`, each listed in its `MDOC-PATCH.md`.

## Focus and theme

- Every control is reachable and activated by keyboard alone on both
  platforms, without a screen reader or Full Keyboard Access.
- Text reaches 4.5:1 and essential non-text 3:1 on every ground it is drawn
  on, in both themes; a new colour joins `style::contrast_tests` in the same
  change.
- Animation honours Reduce Motion and refreshes at most 30 fps.
