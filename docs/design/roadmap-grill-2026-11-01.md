# Roadmap grill after 0.3.0 (2026-11-01)

The maintainer tagged v0.3.0 from `main` (release commit `be899f9`) and
started the release build, then asked to groom the ROADMAP. This records
what was agreed, round by round.

## Facts at the start

- 0.3.0 went stable from rc.1 plus 106 commits, with no rc.2 and no
  acceptance run: the newest platform evidence is 2026-10-27. The shelf's
  Done-when (acceptance section 12 on rc.2) never ran. GUI rows wait on
  #254 (0.3.0 hub and the open-issues run) and #353 (UI polish run).
- #301 (addresses, dates of birth) is open in the 0.3.0 milestone; the
  readiness part landed in #351.
- Done but still listed: iOS item 2 (polish PRs #284–#289, #298; P12, P13,
  P16 unticked), Android item 3 (#314; the Windows runner needs
  reinstalling), Android items 5 and 7 (#322; not yet run on Test Lab),
  Engineering's Dev Drive move (#255). "Release runs reuse `main`'s CI"
  missed rc.1; the v0.3.0 tag run repeats CI.
- Android floor: FP16 misses the 60 s budget on every phone; 4 threads is
  already fastest and XNNPACK can't load the export. FP32, INT8 and
  big-core pinning are untried on a phone (#349).

## Round 1

1. **0.3.0 acceptance: a targeted run on the release build.** Section 12
   (shelf), the rows on #254 and #353, and install, upgrade and keychain
   smoke on both platforms, against the v0.3.0 release build. Must-fix
   fails become 0.3.1; the run's evidence closes #254 and #353. A full pass
   a day after release costs more than it finds; skipping leaves the
   headline feature unchecked.
2. **0.4.0: polish PII recognition.** The models are not perfect, so 0.4.0
   designs and implements an elegant way to fix recognition flaws on the
   fly (the design is grilled from round 2 on). It takes #301 and the
   Later [Detection] items it covers.
3. **Three tracks in parallel, each released when ready.** iOS 0.1 is the
   first ship target (it needs only the maintainer's Xcode sign-in,
   TestFlight upload and device rows); hubs work on 0.4.0 meanwhile;
   Android follows its floor.
4. **Android floor stop rule.** Measure FP32, INT8 and big-core pinning on
   the Pixel 8 and Galaxy A55. If none fits the 60 s budget, Android 0.1
   ships rules-only (identifiers, OCR, marking names by hand), as iOS does
   below its floor, and Find names on Android goes to Later.
5. **Cleanup.** Delete iOS item 2 once P12, P13 and P16 pass in the
   Simulator; delete Engineering's Dev Drive item; fold Android items 3, 5
   and 7 into one "Android device runs" item (reinstall the Windows runner,
   then `just android-device-test` passes on Test Lab per RAM class); move
   "Release runs reuse `main`'s CI" to Later (one RC per minor makes it
   worth about 20 minutes a release).

## Probe: what the model sees in the demos

`scan_at` on both demos at threshold 0.05 (pinned FP16, a throwaway example
binary, not committed):

- The English home address ("Flat 3, 41 Larch Road, Bristol BS6 9ZY") is
  not proposed at any score; the firm's address is (0.91).
- Neither Russian address is proposed at any score.
- The model has no date-of-birth label; the claim number "6001842/2026"
  shows only as a garbled address span at 0.14.
- "Марина" alone scores 0.29, under the 0.5 threshold.

So lowering the threshold fixes none of #301; rules and propagation do.

## Round 2: recognition in 0.4.0

1. **A correction pipeline after the model, in layers, staged 1+4, 2, 3.**
   Agreed in outline; **its design gets its own grill in a fresh session**
   before code.
   1. *Rule pack:* data, not code (EN, RU, shared): `never` (titles, roles,
      category words, defined terms), `context` (a label word proposes a
      category: "born …", "дата рождения", "Claim No", "Дело №", postcode
      shapes), `shape` (today's `identifying`), `span` (boundary fixes). It
      replaces the hardcoded lists in `mdoc-detect`.
   2. *Propagation in the document:* an applied or marked name's parts and
      variants elsewhere (surname alone, first name alone, initials,
      Russian inflections) become proposals for the same identity.
   3. *The user's own lists:* never-propose from Keep, always-propose from
      a manual mark, per case or user-wide.
   4. *Flaw → fixture loop:* every flaw found becomes a failing fixture
      first, then a fix.
   Not taken: fine-tuning or retraining (no training data, a new pinned
   export, requalification); a second-opinion OS tagger.
2. **Dates: only in a personal context.** Dates next to born, DOB, date of
   birth, дата рождения, д.р., род. are proposed as Date; other dates stay
   (amends ADR 0002's "ordinary dates stay" and the detector's never-Date
   rule). No date shifting.
3. **Claim and case numbers: a new Case category, `CASE_1`.** Proposed only
   in a labelled context (Claim No, Case No, Дело №, номер дела), so statute
   and article numbers stay. A case number finds the party in a public
   court register.
4. **A blocking detection gate in local CI.** The Mac job runs the corpus
   against the pinned FP16 model on every PR touching `mdoc-detect`,
   `mdoc-pii` or the rule pack; each fixture asserts its expected spans; a
   flaw is fixed only when its fixture passes.

## Round 3: the roadmap's shape

1. **0.4.0 items, in order:** (1) the recognition design grill, in a fresh
   session, ending in an ADR (rule-pack format and home, how propagation
   proposes, the user lists' scope and storage, the Case category's UI);
   (2) the detection gate in local CI, alongside (1), with the demos and
   #301's spans as failing fixtures; (3) the rule pack (personal-context
   dates, case numbers, EN/RU addresses, roles and defined terms,
   HTML-entity decoding before detection and readiness); (4) propagation
   in the document; (5) the user's own lists. Items 3–5 wait for item 1's
   ADR.
2. **A sixth item for the other leaks:** hidden Office content (comments,
   deleted tracked changes, properties) and the desktop clipboard's
   concealed flag on restored text. HTML entities ride in the rule pack.
3. **Survey polish as an overnight hub run during 0.4.0** (U13, U18, U19,
   U23, U24, U26), outside the item count: its lanes would otherwise wait
   on the design grill.
4. **0.4.0 keeps ADR 0083 as written:** rc.1, full acceptance on the last
   RC, no stable without it. Current status records 0.3.0 as the
   exception.
5. **Later pruning:** delete the English-demo, HTML-entity, dates-of-birth,
   always/never-lists, role-aware-aliases, hidden-Office-content and
   desktop-clipboard items; U27 (name and patronymic as a merge signal)
   joins the design grill's questions. "Russian case forms on restore" and
   "the next detection language" stay.

## Applied (2026-11-01)

- ROADMAP rewritten from rounds 1–3; milestone
  [0.4.0](https://github.com/lawkitt/mdoc/milestone/7) created and #301
  moved to it.
- iOS item 2 deleted after P12, P13 and P16 passed in the Simulator on
  `main` at `be899f9`: `just ios-test` (iPhone 17, iOS 26.1) passed,
  including the hyphenation and dark-mode 3:1 underline tests (P16);
  `just ios-survey` on the iPad Pro 13-inch (M5) shows Text and Original
  side by side and Identities beside the text (P12); the iPhone 17 survey
  in light and dark and `LargeTextExploreUITests` at the largest
  accessibility size show every screen readable, wrapped rather than
  clipped (P13).
