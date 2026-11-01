# iOS models download separately

Status: Accepted, 2026-11-01.

## Problem

On a phone above the detector floor, every iOS download took both models
at once: the name detector (about 630 MB) and text recognition (about
18 MB). Someone who only wants scans and image-only PDFs read had to fetch
the detector too, from Settings or from the card in the document that
asked for recognition.

## Decision

- **Settings downloads each model on its own.** Every model that is not
  installed has its own Download button with its size. Download all, with
  the total, shows when more than one model is missing.
- **A document's setup card offers what its job needs.** Recognition asks
  for text recognition; Find names asks for the name detector. The card's
  title, wording and size follow that model, and **Download & continue**
  resumes the job once it is installed. When other models are missing too,
  the card also offers **Download all models** with the total, which
  resumes the job the same way.
- **Home's card offers every missing model**, and its wording names only
  what is still missing.
- **The cellular question names the size of what was asked for.**

## Consequences

- A phone can read scans without ever fetching the detector; Find names
  then asks for the detector alone.
- Android still downloads its models together
  ([ADR 0089](0089-android-app.md)); matching this is its own change.
