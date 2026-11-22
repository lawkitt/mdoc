- **Names in the Vault.** `mdoc-vault` now keeps each Project's identities,
  with ids that never change, its Always and Never lists and the user-wide
  ones, and an Unfiled document's own names. Moving a document into or out
  of a Project gives its names the aliases there and rewrites its text, and
  can be undone; deleting a Project moves its documents to Unfiled or
  deletes them. Nothing uses it yet
  ([#1091](https://github.com/lawkitt/mdoc/issues/1091)).
