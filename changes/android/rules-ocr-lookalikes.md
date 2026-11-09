- **OCR-damaged words are no longer proposed as names.** In a Russian
  scan, a common word or abbreviation written in Latin or Greek look-alike
  letters («Ha» for «На» before a date, «Bce», «T.π.») is dropped as a
  person or an organization; a real name with a letter of its own
  («Ивaнов», "Hans", «ООО «Abc»») stays. In an English scan, "PagelD"
  ("PageID"), "IntereStee", "LIU" in "(Full name of company)", "LIZ" after
  a company's name, "V.S. agriculture" and "Netberlaads" are dropped, a
  "Depertuent of Agriculture" is a public body, and a garbled token is no
  tax number ("t e43b7") or identity number ("2808888888R") without its
  label ([ADR 0092](docs/adr/0092-recognition-correction-pipeline.md), part
  of #549).
