- **The mobile core saves and restores AI replies through the Vault.** The
  phones' core saves a reply for a Project or an Unfiled document, lists a
  Project's and a document's replies, restores one with the people it named
  when saved, and deletes it. A new handle for the Restore extension reads
  Projects and documents and writes only new replies, never the Vault's
  manifest; the app picks them up on refresh. Both apps still use the old
  library until they switch
  ([#1091](https://github.com/lawkitt/mdoc/issues/1091)).
