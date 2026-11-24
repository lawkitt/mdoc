- **Delete Project leaves nothing behind.** An import not read yet lands in
  Unfiled before its Project goes, so mdoc stopping between the two no
  longer leaves it naming a deleted Project. An import saved just as the
  Project is deleted moves or goes with the rest, and a delete that fails
  part way shows the documents it already moved in Unfiled. Open in mdoc
  starts in Unfiled, whatever page shows (fixes #1140).
