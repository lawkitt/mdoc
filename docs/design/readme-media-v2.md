# README media v2 — design interview

Status: complete (2026-10-16). Supersedes the screenshot and animation
decisions (3, 10–14) in [readme-polish.md](readme-polish.md).

## Context

PR #41 replaced the README's *Brown v. Board* OCR hero, the UK Model Services
Contract shot and the `demo.webp` animation with eight unretouched captures of a
fictional services agreement (light and dark). Review found:

- capture hygiene: the floating zoom pill covers body text; the notice bell
  shows a "1" badge; the unsaved-changes dot shows; window chrome differs
  between captures (traffic lights vs. blank pill);
- story gaps: OCR of scans is gone from the README; the Share panel (Copy for AI,
  reply restoration, cases) was captured but not shown; the demo has no hard
  detection cases and does not read as a legal document.

## Decisions

1. **Audience.** Primary reader is a lawyer or paralegal deciding whether to
   trust mdoc with client files. The first screen is written so a casual visitor
   gets it in five seconds: one hero showing the whole loop, then proof.
2. **Core story: the round trip.** Use ChatGPT or Claude on client documents
   without sending client names: mdoc swaps names for aliases and puts the real
   names back in the AI's reply. Three supporting claims always accompany it:
   - works 100% locally — documents never leave the computer;
   - reads scanned PDFs (OCR);
   - reads Word and other office documents.
3. **Hero format.** A 15–20 s looping video of the whole round trip
   (scan → review → copy for AI → paste reply → names restored), hosted as a
   GitHub-uploaded MP4 so it plays inline; an animated WebP in `docs/images/` is
   the fallback if in-repo hosting is preferred. Stills follow below it.
4. **Demo material.**
   - A new fictional *legal* document with hard cases (title forms, first name
     alone, possessives, address, phone, reference number, date of birth), and
     one deliberate detector miss added manually during review to show the
     review step honestly.
   - *Brown v. Board of Education* (347 U.S. 483, public domain) stays as the
     OCR proof.
5. **PR #41.** Fix capture hygiene (no zoom pill over text, no bell badge, no
   unsaved dot, consistent focused window chrome), add the Share capture to the
   README, then merge. Q3/Q4 work is a follow-up PR.
6. **Orphaned assets.** Delete `demo.webp`, `pdf-scan-*.png` and `docx-*.png`
   once unused (git history keeps them); this note supersedes the old decisions.
7. **Windows.** No Windows shot in the README hero (one platform for
   consistency); one Windows capture goes in `docs/product-media/`.

8. **README copy.** Tagline and intro:

   > **Use AI on client documents without sending client names.**
   > mdoc reads scanned PDFs, Word and other office documents, swaps names and
   > other identifying details for consistent aliases, and puts the real names
   > back into the AI's reply. Everything runs on your computer. Nothing is
   > uploaded.

   The round trip leads; "nothing is uploaded" closes.
9. **Fix the warts in the app, not at capture time.** Users see them too.
   - The editor zoom pill must not sit over body text at rest (fade after idle,
     show on hover or scroll).
   - The notice bell renders whenever `notices > 0` (`ui::notice_bell`); a
     freshly opened DOCX must not raise a badge for the expected "preview is
     approximate" limitation — that belongs in the Source header, not the bell.
   This is a separate app PR; recapture follows it.
10. **Demo document: fictional employment settlement agreement** (UK). Embedded
    hard cases: "Ms Bennett", "Maya", "Bennett's"; a second person sharing the
    surname ("Daniel Bennett") whose alias must stay distinct; a street address,
    a UK phone number, a claim number, a date of birth, a National Insurance
    number. Deliberate miss: the nickname "Mia", added manually during review.
11. **Hero: theme-aware animated WebP in the repo** (`<picture>` with light and
    dark files, about 2 MB each, looping, no controls). No GitHub-uploaded MP4:
    it cannot switch theme and needs a manual browser upload.
12. **Stills under the hero, in order:** (1) *Brown v. Board* scan beside its
    Markdown (OCR proof); (2) Review panel showing the hard cases; (3) the
    restored AI reply tab (the payoff, not the Share panel's buttons). Side by
    side and Word conversion live in `docs/product-media/` only.
13. **Website parity.** lawkitt.com/mdoc updates in the same cycle; the README
    WebP and the website films render from the same fframes source in
    `videos/mdoc/` (web repository).

14. **Sequencing: one capture pass.** PR #41 becomes a draft. The app fixes
    (decision 9) ship in beta.11; then a single capture pass with the
    settlement agreement and *Brown v. Board*, the WebP hero and the README
    copy land in PR #41. Main keeps its current README until then.
15. **Source header label.** The Source preview header in beta.10 still reads
    "Original · <file>", contradicting ADR 0048 and CONTEXT.md (UI label
    **Source**). Fixed in the same app PR as decision 9.
16. **Lean `docs/product-media/`.** A short README (what each capture shows,
    how to reproduce it), the capture checklist, the demo-document generator
    and its inputs. No SHA-256 inventory (git records the bytes) and no notes
    about the website repository's internals.
17. **Capture repeatability: a demo launch mode** (for example `mdoc --demo`)
    that opens the fixtures at fixed zoom, sidebar collapsed, notices quiet and
    a fixed window size, so a full recapture takes minutes. Its own small PR
    after the app fixes. Fully scripted CI capture is out of scope (GPU window
    on CI runners).

## Amendment (2026-10-16, beta.11 capture pass)

- **Demo document:** the README uses the fictional *letter before claim* shared
  with lawkitt.com (decision 13, one source for site and README) instead of the
  settlement agreement (decision 10). In beta.11 the settlement's hard cases
  expose [known issues](../../ROADMAP.md#known-issues) 3 and 4 (surname-only
  mentions are not linked; adding "Mia" by hand would split an identity). It
  returns once those are fixed. The deliberate miss is now the letter's
  Southampton address, added by hand on camera.
- **Hero:** theme-aware animated WebP as decided (decision 11), 1.2 MB each,
  rendered from the website's scene library.
- **Stills:** OCR (*Brown v. Board*), Review and Restore as decided (decision 12).
  The OCR still shows one line promoted to a heading (known issue 1).
- **Repeatability (decision 17):** met without a demo launch mode, by an isolated
  capture profile seeded through `session.json` and scripted input
  (lawkitt/web `videos/mdoc/capture/`).

## Terms

Product terms follow [CONTEXT.md](../../CONTEXT.md): *Copy for AI*, *Restore*
(UI: Paste AI Response), *Case*, *Source preview*. README-only wording:

- **Round trip** — marketing name for the AI analysis handoff plus the AI
  analysis return journey: pseudonymize → Copy for AI → paste the AI reply →
  real names restored in a new tab.
- **Capture hygiene** — what makes a screenshot publishable: focused window
  with traffic lights, no transient overlays over text, no notice badge, no
  unsaved-changes dot, pointer outside the content, current release build.
