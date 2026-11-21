- **Russian CSVs import correctly.** A CSV saved by Excel in Windows-1251,
  or one in KOI8-R, now imports as Russian text instead of garbled Latin
  letters, so the PII scan finds the names in it. UTF-8 and Western European
  CSVs import as before (fixes #1023).
