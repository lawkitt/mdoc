# Privacy and network

Your documents never leave your computer. Opening, converting, recognizing and
pseudonymizing all run locally. mdoc keeps what you import until you delete
it: each document, its original file and its replacements, encrypted in the
Vault under a key the macOS Keychain or Windows data protection holds.
Settings › Vault › Show Recovery Key shows that key as a recovery key, with
no Copy button, and mdoc never writes it to a plain file; keep it to open the
Vault on a new computer. A document's notes stay on your computer: Copy for AI
and Share never include them. mdoc goes online only for:

- **Model downloads**, after you approve them: the text recognition runtime and
  models from GitHub, and the pseudonymization model from Hugging Face. Each file
  is checked against a pinned SHA-256 digest. After that mdoc works offline.
- **The update check** at startup: one anonymous request to the GitHub Releases
  API, carrying no document data. A dot on the Settings button and a banner
  tell you when a newer release exists. Turn the check off in
  **Settings → Updates**.
- **Updates**, only after you click **Download and install** (macOS): the new
  version's disk image from GitHub, checked against the release's `SHA256SUMS`
  and required to carry the mdoc developer's Apple signature. It replaces the
  installed app when you restart or quit mdoc. On Windows mdoc never downloads
  updates; **View release** opens the release page.

Text reaches an AI tool only when you paste it there yourself, or when an AI
app reads it through the [AI connector](#ai-connector-preview), and that tool's
own data terms then apply.

When mdoc stops on an error it can record (a panic in its own code), it offers
a masked crash report at the next launch and sends it only if you choose to. On Windows, mdoc keeps itself
out of Windows Error Reporting, so a crash files nothing with Microsoft.

> [!WARNING]
> Pseudonymization finds most identifying details, not all of them. Dates,
> amounts and context are not replaced and may still identify the parties.
> Review the whole document before you share it; mdoc does not guarantee that
> the result is anonymous.

## AI connector (preview)

Settings › AI connector › **AI connector (preview)** is off by default. With
it off, nothing listens and no AI app can reach mdoc. With it on, Claude
Desktop or ChatGPT's desktop app can read from mdoc through the `mdoc-mcp`
connector, on your computer. Settings › AI connector adds it to Claude
Desktop, or to ChatGPT, where only Work threads call it and Codex reads the
same setting.

- **One Project at a time.** An AI app reads only the AI project: the one
  Project you share, from the sidebar, a Project page, Ask, or the card mdoc
  shows when an AI app asks. Nothing is shared when mdoc starts, moving
  between documents never changes what is shared, and **Stop sharing** ends
  it.
- **What a connected AI app can read:** the shared Project's documents,
  each by a label (the first heading of its reviewed text, or "Document
  N", never its title), with their size and whether each is Ready; a Ready
  document's text, with aliases in place of the names you replaced; and,
  for a document that isn't Ready, only that it needs review. Each read
  checks the document against its Project's names again.
- **What it never gets:** original files, the replacement mapping, restored
  replies, notes, documents excluded from AI, other Projects or Unfiled
  documents. A name that detection and your review both missed stays in
  the text and its heading, and can reach the app, as with Copy for AI.
- **Ask** opens the AI app with a prompt naming the document by its label
  and id; nothing is sent until you send it.
- **Replies:** an AI app can deliver its answer as an AI reply of the shared
  Project or as a new document. mdoc restores the names on your computer,
  and nothing restored goes back to the app.
- **AI activity:** each read of a document's text and each delivery is
  recorded in the Vault with the app and the time, on the Project page's AI
  activity list. Listing the documents and their labels is not recorded.
- **Programs running as you can read the same.** The connector listens on a
  socket (macOS) or named pipe (Windows) that only your user account can
  open, with no password. Any program running under your account can
  connect to it and read what a connected AI app can. Stop sharing, or turn
  the preview off, when you aren't using it.
- **Notifications** name only the AI app and the document's label.
- **The companion window** (macOS) floats over other apps and shows the
  latest reply with real names. It asks macOS to keep it out of screenshots
  and screen sharing, which some capture tools ignore, so the reply shows
  only while the pointer is over it.

## iPhone and iPad

The iOS app ([ADR 0085](adr/0085-mobile-apps.md)) collects no data: no
account, no analytics, no tracking. Its App Store privacy label is "Data
Not Collected", and this section is its privacy policy.

- **Network:** only the model downloads you start: text recognition from
  GitHub and the name detector from Hugging Face, each checked against a
  pinned SHA-256 digest. On cellular mdoc asks first. There is no update
  check; the App Store updates the app.
- **On the phone:** mdoc keeps what you import until you delete it:
  documents, their originals and Projects, encrypted in the Vault in mdoc's
  own storage, which iCloud and device backups exclude, as they do models.
  The Vault is unreadable while the phone is locked, and its key never
  leaves this device. A new phone starts empty.
- **Sharing:** text leaves mdoc only when you share or copy it, and then
  with aliases in place of the names you replaced.
- **Shortcuts (preview):** with Settings › AI connector (preview) on,
  Pseudonymize text adds its text as a new document and opens it for
  review; it returns no text, so nothing leaves that you have not
  reviewed. Restore text saves nothing; its output, with the real names,
  goes wherever the shortcut sends it next.
- **Crash reports** stay on the phone and show at the next launch. Beta
  testers may also send crash reports and feedback through Apple's
  TestFlight, under TestFlight's terms; never attach client documents.

## Limitations

- **Copy Markdown** copies the current text, including unsaved edits.
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
