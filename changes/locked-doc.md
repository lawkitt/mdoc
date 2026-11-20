- **Password-protected Word 97-2003 files import.** The conversion card asks
  for a locked DOC file's password, as for a DOCX, and asks again if it's
  wrong. mdoc unlocks the file in memory and never stores the password or
  writes an unlocked copy to disk. DOC files from Word (RC4 CryptoAPI) and
  LibreOffice (RC4) both open (part of
  [#1007](https://github.com/lawkitt/mdoc/issues/1007)).
