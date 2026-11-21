- **Password-protected Word, Excel and PowerPoint files import.** The
  conversion card asks for a locked DOCX, XLSX or PPTX file's password, and
  asks again if it's wrong. mdoc unlocks the file in memory and never stores
  the password or writes an unlocked copy to disk, so a locked DOCX shows no
  preview. RC4 and rights-managed files still need an unlocked copy
  (part of [#1007](https://github.com/lawkitt/mdoc/issues/1007)).
