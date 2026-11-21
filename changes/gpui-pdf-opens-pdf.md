- **The Original viewer opens a PDF from memory, with a password.**
  gpui-pdf takes a document's bytes and an optional password, and an
  encrypted PDF now gets its form fields, the display fix-up and hidden
  comment annotations like an open one, all in memory; nothing decrypted is
  written to disk. The app's locked-PDF card comes with password-protected
  PDF imports ([#1024](https://github.com/lawkitt/mdoc/issues/1024)).
