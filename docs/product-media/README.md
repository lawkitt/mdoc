# Product media

The README images come from scripted window captures of the released mdoc
0.1.0-beta.11 (2026-10-16), the same pass as the lawkitt.com/mdoc gallery and
films. Decisions: [README media v2](../design/readme-media-v2.md).

## What each capture shows

| Capture | Shows | In README |
| --- | --- | --- |
| `hero-{light,dark}.webp` | The round trip as a 20 s loop: proposals, one identity's mentions, Apply, an AI chat that sees only aliases, the restored reply | Yes |
| `ocr-{light,dark}.png` | *Brown v. Board of Education* scan recognized into Markdown beside the page | Yes |
| `review-{light,dark}.png` | Review panel with 14 identities in the fictional letter before claim | Yes |
| `restore-{light,dark}.png` | An AI reply pasted back, with the real names restored | Yes |
| `compare-{light,dark}.png` | Original and pseudonymized text side by side | No |

The PNGs keep the window's transparent rounded corners, so they sit cleanly on
GitHub's light and dark backgrounds.

## Inputs

- **Letter before claim** — a fictional pre-action letter (people, companies,
  addresses, a phone number, emails, an IBAN). Its address in Southampton is a
  deliberate detector miss, added by hand during review. Sources and the
  alias-only AI reply live in lawkitt/web `videos/mdoc/fixture/`.
- *Brown v. Board of Education*, 347 U.S. 483 (1954), first page of the public
  domain slip opinion, for the OCR capture.
- [Settlement agreement.md](Settlement%20agreement.md) and its
  [.docx](Settlement%20agreement.docx) (`python3 make-demo-docx.py`, needs
  `python-docx`) — a fictional UK settlement with hard cases ("Ms Bennett",
  "Maya", "Mia", a second Bennett). It reproduces [known issues](../../ROADMAP.md#known-issues)
  3 and 4 and returns to the README once those are fixed.

All people, companies, addresses and numbers are invented; phone numbers are in
Ofcom's drama range and emails use `.example` domains.

## Reproduce

From a lawkitt/web checkout, with this release installed in `/Applications`:

```sh
videos/mdoc/capture/all.sh
```

It runs mdoc in an isolated profile (your own session and cases are untouched),
drives the window, and writes stills, clips and films. The README files are the
`review`, `restore`, `compare` and `court-ocr` stills resized to 2000 px, and the
`detect,popup,apply,chat,restore` cut encoded as WebP (1280 px, 12 fps,
`img2webp -lossy -q 72`). After a UI change, update control positions in
`videos/mdoc/capture/ui.env` (see that repository's `videos/mdoc/README.md`).

## Capture checklist

1. Use the current release build, not a development build.
2. Window 1440 × 900 for every capture, focused (traffic lights visible),
   sidebar open with only the demo tabs.
3. Capture each view in light and dark (the session sets mdoc's theme).
4. No notice badge, no update card, no spelling marks, no floating control over
   text; pointer outside the window. The unsaved-changes dot appears only once a
   document has really been edited (after OCR or Apply).
5. Do not retouch or composite UI or document text.
6. For the restore capture, use a short reply written with the aliases (for
   example a summary of the letter and next steps), pasted with
   **Paste AI Response**.

Detection can miss identifiers. These captures are demonstrations, not
detector-quality benchmarks.
