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

- On desktop every DOCX renders in memory: the worker takes the document on
  stdin and returns the PDF on stdout, and the pane opens it from those
  bytes. Nothing of it reaches disk, so an unlocked password-protected DOCX
  shows its Original, and a reopened one asks for its password there
  (#1024). iOS still renders into a file and shows no Original for an
  unlocked DOCX until it renders in memory too.
- docxide is bumped at the start of each desktop milestone, with the DOCX
  fidelity check; no fork unless a patch is unavoidable.
- Upstream docxide issues carry synthetic fixtures only, never a user's
  document.
- The currency-sign fallback stays until a test shows the bundled fonts draw
  ₽ and the newer signs.

## Images

- An image's Original is the PDF the import's image builder makes of it
  (`mdoc-import`'s `image_pdf`), the same PDF that OCR reads, so the page
  shown is the page read. It is built and drawn in memory, never written.
- The EXIF orientation turns the page through its content matrix; a JPEG of
  4,500 pixels or less is embedded as it is, without its metadata segments.
- A TIFF shows every page, each at its own resolution (a fax in normal
  mode keeps its shape); Group 3 and Group 4 fax pages are embedded as the
  fax coded them, so the page shown and the page OCR reads are the fax's.
- Metadata (EXIF, GPS, XMP, camera, dates, ICC profiles, comments) never
  reaches the PDF, so it never reaches the Markdown and isn't counted.

## Phones

- Android follows iOS as it reaches the Original pane; TalkBack reaches each
  comment marker right after its line.
