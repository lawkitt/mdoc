# Locked Office fixtures

Password-protected Office files saved by the real applications, for the
Done-when checks of ROADMAP "Import gaps" items 4 and 5 (#1007). The text is
fictional. Each locked file has an `-open` twin, saved by the same app with
the same text and no password, so a test can check that the right password
gives the same Markdown as the unlocked file.

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

To rebuild them on Windows with Microsoft 365, run
`pwsh -NoProfile -File tests/fixtures/locked-office/make-microsoft-office.ps1`.
Rebuilt files differ byte for byte, because of random salts and timestamps.

## Contents

- **Word, DOCX and DOC:** four paragraphs. "Engagement letter"; "Client:
  Ivan Petrov, 12 Baker Street, Springfield."; "Matter: lease dispute with
  Northwind Holdings Ltd."; and one Cyrillic line, "Клиент: Иван Петров, ул.
  Садовая, 12."
- **Excel:** one sheet with labels in A1:A4 (Client, Address, Matter, Fee)
  and values in B1:B4. The fee, 1200, is stored as a number.
- **PowerPoint:** one slide titled "Engagement letter", whose body holds the
  other three lines.
