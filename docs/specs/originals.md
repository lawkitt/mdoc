# Originals

How mdoc shows a source document beside its Markdown: the DOCX rendering,
its comments and hidden text, and PDF annotations, on desktop and phones.
Terms are in [CONTEXT.md](../../CONTEXT.md).

## Comments

- Comments are never scanned or pseudonymized and show as written in every
  view, Pseudonymized included, because nothing carries them out of the
  device: they never enter the Markdown.
- Cards are read-only because mdoc never writes the source file: no reply,
  resolve or delete.
- No "Copy all comments", because it would carry unscanned names out in bulk;
  a card's text copies like the page's text.
- mdoc keeps no comment parser of its own: the forks (AnyDoc, pdf-inspector)
  extract comments and annotations, and every count and card comes from that
  one extraction.
- OCR reads a scanned page without its comment annotations, so their text
  never reaches the Markdown; stamps, ink and form widgets stay, because they
  are part of the page.
- The active passage is underlined as well as tinted, so colour is never the
  only cue.

## DOCX rendering

- docxide is bumped at the start of each desktop milestone, with the DOCX
  fidelity check; no fork unless a patch is unavoidable.
- Upstream docxide issues carry synthetic fixtures only, never a user's
  document.
- The currency-sign fallback stays until a test shows the bundled fonts draw
  ₽ and the newer signs.

## Phones

- Android follows iOS as it reaches the Original pane; TalkBack reaches each
  comment marker right after its line.
