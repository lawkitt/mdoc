# Recognition language defaults

Status: Accepted, 2026-10-23. Applies [ADR 0065](0065-multilingual-by-design.md)
to OCR; amends [ADR 0006](0006-local-model-settings.md) and
[ADR 0026](0026-first-use-model-setup.md) (what the Settings OCR choice means).

## Problem

In the 0.2.0 rc.1 run the Russian OCR fixture was recognized with the English
default bundle and came out as Latin noise. The OCR card offered only "Choose
model…", which opens Settings and changes the default for every document.
ADR 0065 requires the choice to be a list of languages, not an EN/RU switch.

## Decision

- **Per-document recognition languages.** The OCR card offers the lists the
  engine offers, one per PP-OCR bundle: *English*; *English & Russian*. A pick
  holds for that document until it converts and is never saved. Settings'
  resolution and confidence still apply.
- **Default from the first signal that says anything**, document before
  computer:
  1. the PDF's text layer on the pages that need no OCR: Cyrillic letters
     suggest Russian, Latin letters English (three letters of a script or
     more; fewer are strays);
  2. Cyrillic letters in the file name ("договор.pdf");
  3. a word in the file name naming a language ("russian", "ru", "english",
     "eng", "русский");
  4. a Russian keyboard input source on this computer (macOS Text Input
     Sources, Windows keyboard layouts). An English keyboard says nothing.
  5. Otherwise the Settings OCR choice (English unless changed).

  Latin file names are no signal on their own: Russian documents are often
  named in transliteration ("dogovor.pdf").
- **Suggestions map to the closest offered list**: the first list holding
  every suggested language, else the one holding most. The engine maps the
  list to a bundle and names any language that bundle cannot read.
- **One short line on the card says why**, e.g. "English & Russian: the file
  name is in Russian", "English: the text layer has no Russian text",
  "English: picked for this document". English UI text stays inline
  (ADR 0065).
- **Only the Settings model's readiness is shared between tabs.** A document
  using another bundle shows that bundle's setup in its own card, and its
  setup or run failures stay with it.

## Invariants

- Detection reads only what the document view already has (text-layer
  letters from conversion, the file name) and the input source list; it
  never reads document content beyond that, sends anything or downloads.
- Tests inject the input-source signal; they never read the machine's
  keyboards.

## Consequences

- The Settings OCR choice is the default for documents that suggest no
  language, and the bundle Settings downloads and repairs.
- An OS engine or another qualified language adds an offered list and a
  mapping, not a new control.
