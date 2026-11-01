- **Names in link targets are found.** A scan reads `%20` and percent-
  encoded letters in link and image targets as text and an underscore
  between letters there as a space, and a name it finds is also proposed
  where the document spells it so ("clients/Liora%20Nesket/",
  "Alice_Morgan.png"), as written (fixes #401 and #376;
  [ADR 0092](docs/adr/0092-recognition-correction-pipeline.md)).
