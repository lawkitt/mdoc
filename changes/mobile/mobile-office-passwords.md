- **Password-protected Word files import.** A locked DOCX asks for its
  password on the document and imports once Unlock opens it; a wrong
  password asks again. mdoc unlocks the file in memory and never keeps the
  password or an unlocked copy, so on iOS a locked DOCX shows no Original
  page and its comments count in the left-out line
  (part of [#1007](https://github.com/lawkitt/mdoc/issues/1007)).
