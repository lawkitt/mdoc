- **Readiness checked when text leaves.** `mdoc-vault` decides whether a
  document may go to an AI at the moment it is read for Copy for AI, Share,
  the shelf or MCP: it must have been saved while Ready and hold none of
  its Project's names as written, so a name another document adds makes it
  not Ready. The text for the AI is built then and never stored. Nothing
  uses it yet ([#1091](https://github.com/lawkitt/mdoc/issues/1091)).
