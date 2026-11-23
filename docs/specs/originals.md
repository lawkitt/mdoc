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

- On desktop and iOS, DOCX previews render in memory. The desktop worker
  takes the document on stdin and returns the PDF on stdout. On iOS the
  core renders in process and PDFKit opens the PDF bytes. An unlocked
  password-protected DOCX shows its Original, and a reopened one asks for
  its password there (#1024).
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
- On iOS a locked PDF's Original unlocks in memory with the password that
  imported it, and asks in its pane once the document opens again. Android
  shows no locked PDF's pages, because its renderer takes a password only
  from Android 15.
- A photo's or camera page's Original is the image itself, drawn natively
  and upright. The copy kept is the core's `without_metadata` rewrite
  (JPEG, PNG, WebP, GIF: same pixels, only the orientation left) or, for
  HEIC and what the core doesn't read, the phone's own upright JPEG. The
  phones refuse TIFF for now (#1064).
  On Android, picked photo bytes are sanitized in memory before mdoc
  writes the kept copies. The camera writes its shot to a raw file under
  `no_backup/capture/`, outside `originals/`, which is cleaned into the
  scan's stage and deleted on return; leftovers go when a process starts.
  A scan in progress belongs to the app and is saved with Home's state, so
  a destroyed activity or a stopped process comes back to it; its
  `scanning-` stage is kept for a day at a restart. Cancelling Add deletes
  the staged copies. Staged files wait in `vault-intake/`, beside the
  Vault; a document's first save keeps its original in the Vault and its
  staged files go. On restart only pending imports and scans in progress
  keep their staged files. A `pending.json` that can't be read is moved
  aside (`pending.json.corrupt`) and clears nothing.
- A saved document's original is decrypted from the Vault when its pane
  first shows it: into memory up to 16 MiB (Android draws a PDF from an
  anonymous memory file), else into `vault-original-view/` in the cache,
  wiped when the document closes and at launch (#1091 Decision 24).
- Photos picked together are one document. The core appends one photo at a
  time to an in-memory PDF and applies the 256 MiB import limit to the
  combined image files and the resulting PDF.
