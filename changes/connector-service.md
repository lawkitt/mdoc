- **The AI connector's service: the AI project and reads.** Behind a new
  setting, "AI connector (preview)", off by default, a service holds the
  one Project shared with AI apps (none until mdoc shares one) and lists
  its documents by AI label, size and state. It returns a Ready
  document's text, its "needs review" state, or above 60,000 characters
  an outline with numbered parts. Every read checks the document against
  its Project's names again, never reaches another Project or Unfiled,
  and never names a document by its title. The connector joins the suite
  of ways text leaves mdoc. Nothing of it runs or shows yet (Milestone 2
  item 5, [#1164](https://github.com/lawkitt/mdoc/issues/1164)).
