# Recognition grill for 0.4.0 (2026-11-01)

The maintainer asked to grill 0.4.0's recognition work so that it ends as a
set of long-running tasks a hub runs for hours with the maintainer away:
agents find flaws themselves, fix them, and go on. This is ROADMAP 0.4.0
item 1 (the design grill), widened to how the work runs autonomously. It
records what was agreed, round by round.

## Facts at the start

- **Pipeline today** (`mdoc-detect::scan_timed`): GLiNER2 PII (pinned
  FP16) per window of at most 128 words, five label families, threshold
  0.5; each span passes `identifying()` (hardcoded `TITLES_AND_ROLES`,
  `DETERMINERS`, `RU_ROLE_STEMS`, per-category shape checks); then
  `structured::scan` adds rule hits (email, INN, SNILS, OGRN). Rules win
  over overlapping model hits. `Recognizer` is `Model | Email | Inn | Snils
  | Ogrn`. `guess_category` (manual marks) has its own `LEGAL_FORMS`,
  `date_shaped` and `name_shaped`.
- **Corpora:** `tests/fixtures/pseudonymization/corpus.json` (18 cases,
  renders' deterministic proposals) and `hybrid/corpus.json` (33 cases,
  CC0, `⟦CATEGORY|text⟧` gold, a frozen 12-case holdout). The only
  automated check against them is `structured::captured_corpus`, rules
  only; nothing in CI runs the model against a corpus. The qualification
  tool (`tools/pseudonymization`) runs the model outside the app graph.
- **Models on disk:** FP16 and FP32 on the Mac and under
  `%LOCALAPPDATA%\mdoc\pseudonymization` on the Windows PC, so both local
  CI runners can run the detector. Linux has no automatic scanning
  (ADR 0016).
- **The demos' misses** (2026-11-01 probe): no home address in either
  demo at any score, no date-of-birth label, the claim number only as a
  garbled 0.14 address span, "Марина" alone at 0.29. Lowering the
  threshold fixes none of it.
- **Mac capacity now:** 57 GiB free, swap 3.1 GiB used (the fleet's
  builder limit is 2 GiB), so the Mac takes at most one builder plus the
  `local/macos` runner until swap drops.
- **Agreed before** (roadmap grill, round 2): a correction pipeline after
  the model in layers (rule pack, propagation, the user's lists, flaw →
  fixture); dates only in a personal context; a Case category (`CASE_1`)
  in a labelled context; a blocking detection gate in local CI. Not taken:
  fine-tuning, a second OS tagger.

## Round 1 (all agreed as recommended)

1. **This session is item 1.** It ends with ADR 0092 (recognition
   correction pipeline), ADR 0093 (flaw hunt loop) and ROADMAP items 2–6
   rewritten as hub-ready briefs.
2. **Hunters and fixers are separate, on different providers.** Codex Sol
   hunts (review-like work: writes realistic synthetic legal documents,
   runs the detector, reports each miss or false hit with its gold spans)
   on the Windows PC; Opus fixes and opens every PR. The Mac keeps the
   hub, one builder and `local/macos`.
3. **Gold and honesty.** An annotation guide
   (`tests/fixtures/pseudonymization/GUIDE.md`) defines PII and span
   boundaries; hunters build documents from slots so gold is known by
   construction; the fixer audits each flaw's gold against the guide and
   may reject it; a private holdout (`lawkitt/mdoc-holdout`) that no fixer
   sees measures overfitting, run only by the hub at checkpoints.
4. **Strict expected failure.** A hunter's fixture lands as
   `status: known-flaw` with its issue; the gate fails when it starts
   passing until the status flips to `pass`. `pass` never returns to
   `known-flaw` without the maintainer.
5. **Model-output snapshots.** Each fixture's raw model output is
   committed; the correction pipeline runs on snapshots anywhere in
   milliseconds; the Mac gate reruns the model and fails on a stale
   snapshot (as ADR 0056 does for AI replies).
6. **Scoreboard and stop rule.** Recall and precision per language ×
   category × document kind, and the known-flaw count. Recall first,
   precision ratcheted (no new false hits on negatives). A hunter's
   coverage matrix (contract, claim, judgment, power of attorney, letter,
   OCR scan, DOCX-converted, tables × EN/RU); a cell is saturated after
   two rounds with no new kind of flaw. The run stops when all cells are
   saturated and no known flaw is left, or by the fleet's stops.
7. **Autonomy.** Alone: rule-pack data, pipeline code in `mdoc-detect`
   and `mdoc-pii` within ADR 0092, fixtures, the guide's examples. Parked
   as a `decision` issue (the lane moves on): a new category, UI beyond
   ADR 0092, the guide's rules, deleting or loosening a fixture, the
   threshold, the model or its export.
8. **PR size.** One fixtures-only PR per hunt round (5–15 known flaws);
   one fix PR per flaw class.
9. **Rule pack:** TOML compiled in (`crates/mdoc-detect/rules/{shared,en,ru}.toml`,
   `include_str!`, validated by a unit test), sections `never`,
   `context`, `shape`, `span`. `guess_category`'s legal forms and date and
   name shapes move in, so manual marks and the model share one source.
10. **Case and Date in Settings and the popup**, both on by default: "Dates
    of birth" (personal context only), "Case and claim numbers"; Case gets
    the next category colour.
11. **Propagation proposes** (recognizer `Propagation`) variants of an
    applied or marked name, for review; Apply all includes them. A part
    that is also a common word is proposed only capitalised mid-sentence.
12. **The user's lists:** per case by default (Keep → never, manual mark →
    always), plus a hand-edited user-wide list in Settings › Detection;
    both in the keychain-protected store.

## Round 2 (agreed as recommended, Q4 corrected)

1. **Propagated variants link when the candidate is unique.** A variant of
   an applied or marked full name joins that identity as a proposal when
   exactly one full name fits; the user's Apply or mark of the full name
   is the linking action. Two or more candidates keep ADR 0068's separate
   identity and ADR 0080's hint. The popup can unlink one mention. Amends
   ADR 0068 ("proposed, never linked") and ADR 0080.
2. **First names and nicknames.** A first name alone is proposed (linked)
   when exactly one person in the document has it; nicknames ("Мариша",
   "Маша", "Bob", "Liz") come from a small `variants` table in the rule
   pack, only for names already in the document; first names inflect by
   the same suffix approach as surnames. Amends ADR 0068's "a first name
   alone is never proposed".
3. **U27: patronymic and initials as link signals**, unique candidate
   only: first name + patronymic links to the full name; surname with
   every initial matching links; an initial matching a different given
   name breaks the link. Amends ADR 0080's "not signals: initials".
4. **Pipeline order**, at scan time in `mdoc-detect`: model → `never` and
   `shape` filters → `context` rules (DOB, Case, addresses) → structured
   identifiers → `span` fixes → the user's lists; propagation runs in
   `mdoc-pii` after a decision. *Correction:* mdoc targets devices that
   run the model, so the no-model path (`scan_rules`) is not a target; the
   rule pack is designed and tested in the model pipeline only.
5. **Addresses: anchor and extend.** Anchors are postcode shapes (UK, US
   ZIP, RU index) and a street-type word with a house number (Road,
   Street, Flat; ул., проспект, переулок, д., кв., офис); the span extends
   to clause or line boundaries. Negative fixtures hold precision.
6. **The Mac is the reference platform.** Snapshots are made and checked
   there; `local/windows` runs only snapshot-based tests; hunters' Windows
   output is re-confirmed on the Mac by the curator; once per run the hub
   compares Windows and Mac output and files drift. The gate's model run
   has a 5-minute budget on the Mac; past it, a per-PR set and a nightly
   full set.
7. **Fixtures:** one Markdown file each,
   `tests/fixtures/recognition/{en,ru}/<kind>/<id>.md`, front matter
   (`status`, `flaw`, `issue`, `kind`, `source`), `⟦CATEGORY|text⟧` gold,
   snapshot `<id>.model.json` beside it. The hybrid and render corpora
   are imported; their originals stay frozen.
8. **Hunt → PR:** the hunter pushes `windows/hunt-rN` with a findings
   report; an Opus curator (hub sub-agent) audits gold, drops bad cases,
   makes Mac snapshots and opens the fixtures PR. One issue per flaw class
   (label `flaw`, 0.4.0 milestone); fixers take a class and close it.
9. **Holdout:** ~60 documents (30 EN, 30 RU, all kinds, 10 negatives)
   generated before any fix lands, pushed only to private
   `lawkitt/mdoc-holdout`, frozen. The hub runs it every 5 fix merges and
   at the end, logs aggregates only. Falling holdout recall, or holdout
   gains under half the fixture gains over two checkpoints, pauses fixers
   and parks a `decision`.
10. **`just recognize <file.md> [--gold] [--snapshot]`**, an example binary
    in `mdoc-detect` (not shipped): annotated output, and against gold the
    misses, false hits and boundary errors. Built in item 2, before any
    hunter starts.

## Round 3 (all agreed as recommended)

1. **Hidden Office content:** deleted tracked changes dropped (the
   Markdown reads as the document does now, insertions accepted); comments
   dropped, the import note says "N comments not imported"; document
   properties never carried. The lane checks the pinned AnyDoc on a fixture
   DOCX first, then enforces this in `mdoc-import`.
2. **Restored text on the desktop clipboard:** macOS
   `org.nspasteboard.ConcealedType` and `TransientType`; Windows
   `ExcludeClipboardContentFromMonitorProcessing` and
   `CanIncludeInClipboardHistory = 0`. Copy for AI stays a normal copy.
3. **The user's lists:** Keep adds the identity's wordings to the case's
   Never list (undo removes them); a manual mark adds to Always;
   whole-word, case- and whitespace-insensitive; a person's Always entry
   yields propagation's forms; the case page lists both (remove only);
   Settings › Detection holds user-wide Always and Never (add, remove); a
   document outside a case uses the user-wide lists.
4. **Flaws no layer fixes:** preprocessing (windowing, table cells as
   sentences, OCR hyphen joins, entity decoding) is pipeline code a fixer
   may change; still unfixable → `status: model-limit` with the curator's
   reason, listed for the maintainer. Hunt material is synthetic and CC0.
5. **Done-whens** for items 2–6 as written in the ROADMAP.
6. **Lanes and waves.** Wave 1: item 2 on the Mac; item 3's groundwork
   (loader, lists moved, no behaviour change) and item 6 as Opus lanes on
   the PC; the holdout by a Codex thread. Wave 2 when item 2 merges: Codex
   hunters EN and RU, item 3's fix loop, item 4; a curator sub-agent per
   hunt round. Wave 3: item 5 when a lane frees.
7. **`wsl` Claude login:** done by the maintainer during the grill, so
   Opus lanes on the PC run as Desktop SSH sessions on `wsl`.
8. **Hub and budget:** this session becomes the hub once the docs PR
   merges. No new Opus dispatch below 50% weekly Claude left; Codex hunting
   goes on to its own limit. No release or rc.1 in the run; the end report
   says whether 0.4.0 is ready for rc.1.
9. **Setup while the maintainer is present:** the private
   `lawkitt/mdoc-holdout`, labels `flaw`, `decision`, `model-limit`, and
   one issue per known flaw class from #301 and the demos.
10. **GUI:** UI parts merge on light and dark renders; GUI rows go to the
    run's "GUI rows pending (maintainer)" issue. The survey polish run
    stays a separate later hub.

## Applied

- [ADR 0092](../adr/0092-recognition-correction-pipeline.md) and
  [ADR 0093](../adr/0093-flaw-hunt-loop.md); the
  [annotation guide](../../tests/fixtures/recognition/GUIDE.md), whose
  rules follow the decisions above and the ROADMAP's "courts" example for
  Always (courts and public bodies are not proposed by default).
- The category is named **Case number** (`Category::CaseNumber`, token
  `CASE`, aliases `CASE_1`), since the glossary's Case is a saved mapping;
  the new terms joined `CONTEXT.md`.
- ROADMAP 0.4.0: item 1 removed (done here), items 2–6 rewritten with
  these Done-whens.
