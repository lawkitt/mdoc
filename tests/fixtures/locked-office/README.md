# Locked Office fixtures

Password-protected Office files, mostly saved by the real applications, for the
Done-when checks of ROADMAP "Import gaps" items 4 and 5 (#1007). The text is
fictional. Each application fixture has an `-open` twin with the same text,
so a test can check that the right password gives the same Markdown.

The password for every locked file is the published test value
`Petrov-2026`. It protects nothing real.

The folder sits outside `tests/fixtures/import`, whose corpus test converts
every file in that folder and would fail on a locked one.

| File | Saved by | Encryption |
|---|---|---|
| `word-locked.docx` / `word-open.docx` | Word (Microsoft 365, 16.0) | OOXML agile 4.4, AES-256, SHA-512 |
| `excel-locked.xlsx` / `excel-open.xlsx` | Excel (Microsoft 365, 16.0) | OOXML agile 4.4, AES-256, SHA-512 |
| `powerpoint-locked.pptx` / `powerpoint-open.pptx` | PowerPoint (Microsoft 365, 16.0) | OOXML agile 4.4, AES-256, SHA-512 |
| `word-locked.doc` / `word-open.doc` | Word (Microsoft 365, 16.0), Word 97-2003 format | RC4 CryptoAPI (header 4.2), 128-bit |
| `libreoffice-locked.docx` / `libreoffice-open.docx` | LibreOffice 26.8.3 Writer | OOXML standard 3.2 |
| `libreoffice-locked.xlsx` / `libreoffice-open.xlsx` | LibreOffice 26.8.3 Calc | OOXML standard 3.2 |
| `libreoffice-locked.pptx` / `libreoffice-open.pptx` | LibreOffice 26.8.3 Impress | OOXML standard 3.2 |
| `libreoffice-locked.doc` / `libreoffice-open.doc` | LibreOffice 26.8.3 Writer, Word 97 format | RC4 (header 1.1) |
| `comments-locked.docx` | `craft_offcrypto::encrypt_with_spin_count` with 1,000 spins, from [comments.docx](../docx-preview/comments.docx) | OOXML agile |

The application fixtures cover agile and standard OOXML encryption and both
RC4 forms of DOC. They open in Word, Excel or PowerPoint with the password
and are refused with a wrong one. The generated DOCX exercises comment
placement after unlocking in the iOS tests.

To rebuild them on Windows with Microsoft 365, run
`pwsh -NoProfile -File tests/fixtures/locked-office/make-microsoft-office.ps1`.
For the LibreOffice files, start `soffice --headless --invisible --norestore
"--accept=socket,host=127.0.0.1,port=2002;urp;"` and run `make-libreoffice.py`
with LibreOffice's own `program\python.exe`. Rebuilt files differ byte for
byte, because of random salts and timestamps.

## Contents

- **Word and Writer, DOCX and DOC:** four paragraphs. "Engagement letter"; "Client:
  Ivan Petrov, 12 Baker Street, Springfield."; "Matter: lease dispute with
  Northwind Holdings Ltd."; and one Cyrillic line, "Клиент: Иван Петров, ул.
  Садовая, 12."
- **Excel and Calc:** one sheet with labels in A1:A4 (Client, Address, Matter, Fee)
  and values in B1:B4. The fee, 1200, is stored as a number.
- **PowerPoint and Impress:** one slide titled "Engagement letter", whose body holds the
  other three lines.
