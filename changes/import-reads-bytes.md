- **An import reads its file once.** Conversion, the DOCX preview and the
  comments beside it all use the bytes the import read, and the preview's
  worker gets them on its input instead of opening the file itself
  (part of [#1007](https://github.com/lawkitt/mdoc/issues/1007)).
