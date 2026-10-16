# Product media

Screenshots and the README hero come from real window captures of a released
mdoc build. Decisions and rationale: [README media v2](../design/readme-media-v2.md).

> [!NOTE]
> The captures in `docs/images/` still show the earlier services agreement and
> beta.10. They are replaced in one pass after beta.11 ships the zoom pill,
> notice and Source label fixes.

## What each capture shows

| Capture | Shows | In README |
| --- | --- | --- |
| `ocr-{light,dark}` | *Brown v. Board of Education* scan beside its recognized Markdown | Yes |
| `review-{light,dark}` | Review panel with the settlement agreement's proposed identities | Yes |
| `restore-{light,dark}` | An AI reply pasted back, with the real names restored | Yes |
| `compare-{light,dark}` | Original and pseudonymized text side by side | No |
| `source-{light,dark}` | The settlement DOCX as Markdown beside its Source preview | No |
| `hero-{light,dark}.webp` | The round trip as a looping animation | Yes |

## Inputs

- [Settlement agreement.md](Settlement%20agreement.md) — fictional UK employment
  settlement, the review input. It contains deliberate hard cases: "Ms Bennett",
  "Maya" and "Ms Bennett's" for one person; Daniel Bennett, who shares her
  surname and must get a different alias; an address, a phone number, a claim
  number, a date of birth and a National Insurance number. The nickname "Mia"
  is the deliberate miss, added by hand during review.
- [Settlement agreement.docx](Settlement%20agreement.docx) — the same text as a
  Word file, for the conversion capture. Rebuild it from the Markdown with
  `python3 make-demo-docx.py` (requires `python-docx`).
- *Brown v. Board of Education*, 347 U.S. 483 (1954), first page of the public
  domain slip opinion, for the OCR capture.

All people, companies, addresses and numbers in the agreement are invented. The
phone number is in Ofcom's drama range; the email uses `example.com`.

## Capture checklist

1. Use the current release build, not a development build.
2. Window at the same size for every capture, focused (traffic lights visible),
   sidebar collapsed.
3. Capture each view in light and dark, switching the theme inside mdoc.
4. No notice badge, no unsaved-changes dot, no floating control over text, no
   hover or caret marks; pointer outside the window content.
5. Do not retouch or composite UI or document text.
6. For the restore capture, use a short reply written with the aliases (for
   example a summary of the agreement's key dates and obligations), pasted
   with **Paste AI Response**.

Detection can miss identifiers. These captures are demonstrations, not
detector-quality benchmarks.
