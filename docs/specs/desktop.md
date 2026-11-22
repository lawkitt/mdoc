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
