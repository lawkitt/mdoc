# Privacy and network

Your documents never leave your computer. Opening, converting, recognizing and
pseudonymizing all run locally. mdoc goes online only for:

- **Model downloads**, after you approve them: the text recognition runtime and
  models from GitHub, and the pseudonymization model from Hugging Face. Each file
  is checked against a pinned SHA-256 digest. After that mdoc works offline.
- **The update check** at startup: one anonymous request to the GitHub Releases
  API, carrying no document data. A dot on the Settings button tells you when a
  newer release exists; mdoc never downloads or installs it. Turn the check off
  in **Settings → Updates**.

Text reaches an AI tool only when you paste it there yourself, and that tool's
own data terms then apply.

> [!WARNING]
> Pseudonymization finds most identifying details, not all of them. Dates,
> amounts and context are not replaced and may still identify the parties.
> Review the whole document before you share it; mdoc does not guarantee that
> the result is anonymous.

## Limitations

- **Copy Markdown** copies the current text, including unsaved edits.
- Aliases are not saved: the mapping from aliases back to the originals lasts
  only while the document is open.
- The pseudonymization detector misses some details, especially in Russian text
  and in hidden parts of a source document; see the
  [qualification results](../tests/fixtures/pseudonymization/README.md).
- Recognition can misread characters and does not read handwriting; check names
  and figures against the original. See the
  [recognition notes](../tests/fixtures/ocr-qualification/README.md).
- Images inside Word files are not recognized; only PDFs are.
- Embedded images are not carried into the Markdown, and complex tables may
  need correction.
- Text recognition and pseudonymization run on Apple silicon Macs and Windows
  x64 only. Intel Macs, Windows on ARM and Linux can
  [build mdoc from source](development.md#build-from-source) and read documents
  with built-in text.

What is planned next is in the [roadmap](../ROADMAP.md).
