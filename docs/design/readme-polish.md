# README cleanup and polish — design interview

Status: complete (2026-10-11).

## Baseline facts

- README links all resolve; the product link `lawkitt.com/tools/mdoc/`
  redirects to `lawkitt.com/mdoc/`.
- "Windows runtime and native acceptance checks remain outstanding" is stale:
  Windows x64 OCR passed on the owner's PC and in the release smoke tests.
- "Right-click → Open" no longer bypasses Gatekeeper on macOS 15+; unnotarized
  apps are allowed in System Settings → Privacy & Security → Open Anyway. The
  release notes written by `release.yml` repeat the old instruction.
- No screenshots or badges exist.

## Decisions

1. **Audience: users first, then contributors.** Order: what it does, install,
   privacy and network, limitations; then build from source, development,
   project docs, license.
2. **Website** (`lawkitt.com/mdoc/`) is linked once near the top as the
   product overview; the README stays factual and practical.
3. **Screenshots:** two public legal documents found online — a DOCX and a
   scanned PDF — each captured in dark and light theme with the sidebar
   collapsed and both Markdown and Original panes open (four images). The best
   one goes in the README; the rest are for the website's mdoc page.
4. **Pseudonymization demo:** an animation made with fframes (see 13), not a
   screen recording; the website keeps its own CSS workflow animation.
5. **Badges:** CI status, latest release (including pre-releases), license.
6. **Privacy and network** section lists everything mdoc sends: model and
   runtime downloads (only after consent, from GitHub, checksum-verified) and
   the update check (on by default, can be turned off). Documents never leave
   the computer.
7. **Limitations** as a short list (Copy includes unsaved edits, aliases not
   saved, detector misses, no OCR for image-only DOCX), linking evidence; the
   pseudonymization warning stays prominent.
8. **No project status in the README**; it states what is supported and links
   ROADMAP.md for status.
9. **First launch:** macOS "System Settings → Privacy & Security → Open
   Anyway"; Windows "More info → Run anyway"; no Terminal workaround. The
   release-notes text in `release.yml` is corrected the same way.
10. **Screenshots in the README:** two theme-aware `<picture>` images (light
    and dark from the reader's theme): the scanned *Brown v. Board of
    Education* (347 U.S. 483, public domain) OCR'd beside its original, at the
    top; the DOCX (UK Model Services Contract) beside its preview in the
    features area. Captured from the beta.2 app, sidebar collapsed, window
    active, no caret or hover marks; the OCR'd heading was corrected by hand.
11. **Image files:** 1600 px wide optimized PNG in `docs/images/`; full-size
    3420×2136 originals kept outside git for the website.
12. **Attribution:** the DOCX image is captioned "Crown copyright, Open
    Government Licence v3.0", also recorded in NOTICE.md; the scan needs none.
13. **Demo animation:** rendered with fframes from `videos/mdoc` in the web repository
    (`--cut readme`): a scanned page recognized into Markdown, then a
    fictitious services agreement whose names, email and phone number are
    proposed, applied with **Apply replacements**, and listed as aliases; 20 s,
    looping, in the website's dark style. A screen recording from the app was
    tried and dropped in favour of the designed video.
14. **Animation format:** animated WebP, 1200 px wide at 15 fps (1.9 MB), in
    `docs/images/demo.webp`. The other fframes cuts (recognition,
    pseudonymization, AI handoff and a combined hero) are MP4s for the website.
