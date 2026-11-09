# Recognition run 3 grill (2026-11-09)

A grill of 0.4.0 recognition run 3 ([#471](https://github.com/lawkitt/mdoc/issues/471))
after both hubs stopped: are we moving in the right direction, and is it the
best way to get there. Amends the run's decisions on #471 and
[ADR 0093](../adr/0093-flaw-hunt-loop.md).

## Facts at the start

- **What run 3 does.** It corrects the pinned GLiNER2 model after the fact
  ([ADR 0092](../adr/0092-recognition-correction-pipeline.md)): a rule pack
  of 315 KB of TOML (`rules/{shared,en,ru}.toml`) plus about 8k lines of
  heuristics and 14k lines of tests in `mdoc-detect` (`rules.rs` is 20,474
  lines). Flaw classes come from synthetic hunts and real probes.
- **Throughput.** About 113 recognition PRs merged from 11-02 to 11-09
  (about 53k lines added). 99 flaw classes filed, 47 closed, 52 open; 50 of
  the open ones are `hub:windows`. Only 4 new classes since 11-04: the run
  has been fixing, not finding. Open classes went from 61 (11-03) to 52.
- **Classes rarely close.** Most PRs are "Part of"; each probe round reopens
  a class with new shapes (#362, #363, #364). Many classes are open-ended
  false-hit lists (#501, #559).
- **Holdout.** Run 3 never ran it, though ADR 0093 asks for a checkpoint
  every 5 fix merges. On main `49b525ce` (2026-11-09): recall 97.6%
  (en 98.3, ru 96.9), precision 94.0% (en 96.2, ru 92.1), 40 of 60
  documents fully pass. Earlier: 63.7% / 66.0% (run 2 start), 91.1% / 92.1%
  (run 2 end). The fixes generalise to unseen synthetic documents.
- **Demos.** All nine `demo-*` fixtures pass, so ROADMAP's demo condition
  is met. 28 `known-flaw` fixtures remain.
- **Real documents** have no repeatable score: probe rounds produced counts
  per issue ("×63"), not a scoreboard.
- **Hubs.** The Mac hub's own stage has no open classes; it gates and
  enqueues Windows' PRs. Much of its time went to queue upkeep: watchers
  went silent twice, setup 504s (#724), the PC queue-runner script test
  (#737), a full PC disk.
- **Rule growth.** The Russian address regex is at 9.39 of the 10 MB regex
  limit.

## Round 1 (all agreed as recommended)

1. **Done means targets, not every class closed.** Run 3 as defined could
   not end: classes reopen with each probe round and false-hit classes are
   open-ended. Done becomes measured targets (set in round 2): misses that
   leak personal data block; everything else is a 0.4.x backlog.
2. **Misses block; false hits by volume.** A miss sends real personal data
   to the AI; a false hit costs one Keep in review. Only miss classes block
   0.4.0. A false-hit class is fixed only when it is frequent in real
   documents (threshold in round 2); the rest wait.
3. **Measure on real documents.** A frozen, private set of probe documents
   gets gold once (annotator plus a fresh confirmer, against GUIDE.md) and
   is scored like the holdout; only aggregates are posted. The holdout
   becomes a scripted step of the gate, so a hub cannot skip it.
4. **Rules for 0.4.0, a time-boxed model spike beside them.** The rule pack
   works and 0.4.0 ships on it. A spike scores a fine-tuned or newer model
   on the holdout and the real-document set, so 0.5 can choose between
   more rules and a better model. Changing the model stays the
   maintainer's decision.
5. **One hub.** The work now lives in one stage, so one hub on Windows
   builds and enqueues; a scripted stack check (main + PRs ahead in the
   queue + the candidate) runs before each enqueue. This lifts Windows'
   "never merge or enqueue". The Mac joins only when a snapshot must be
   recorded locally.
6. **Drain while the grill runs.** Queued and ready work lands; no new
   builds until the remaining branches are decided. #753 merged at 06:41Z;
   #751 and #752 dropped again on setup 504s (#724) and were re-enqueued
   unchanged at 09:02Z.

## Round 2 (all agreed as recommended)

7. **A fresh real-document set.** Probe rounds 1 and 2 already shaped the
   fixes, so scoring on them would flatter. A new round 3 of about 25 EN and
   25 RU documents (same sources and kinds) is collected, given gold once
   (annotator plus a fresh confirmer, against GUIDE.md) and frozen in
   `lawkitt/mdoc-probes`. No fixer sees it; only aggregates are posted.
   Rounds 1 and 2 stay as hunting ground.
8. **Targets (Done for 0.4.0 items 3 and 4).** Provisional; revisited once
   the real set's baseline is known.
   - Real set: recall at least 95% overall and at least 90% in each leaking
     category (PERSON, ADDRESS, IDENTITY, TAX, BANK, PHONE, EMAIL, date of
     birth, CASE).
   - Real set: precision at least 85%.
   - Holdout: no fall below 97.6% recall and 94.0% precision.
   - Both demos stay clean.
9. **A false-hit class is fixed** when it appears in at least 3 real-set
   documents or makes up at least 5% of the real set's false hits. Below
   that it waits.
10. **Triage once.** After the first real-set score the hub keeps miss
    classes (about 16) in 0.4.0 and moves the false-hit classes (about 36)
    to a "Recognition backlog" milestone; a class returns to 0.4.0 when the
    real set shows it meets item 9.
11. **Model spike now, as its own session.** Candidates: GLiNER2 fine-tuned
    on freshly generated synthetic documents (never the fixtures), and one
    or two off-the-shelf multilingual PII models at most twice the current
    size. Measured alone and under today's pipeline, on the holdout and the
    real set, with speed on the Mac and an iPhone-class device. One day,
    nothing merged; a report and a decision for the maintainer.
12. **Hub mechanics.** `scripts/hub-gate.sh <PR>` builds main + the PRs
    ahead in the queue + the candidate, runs the recognition tests, and runs
    the holdout when its private path is set. #471 closes with a summary;
    a run 4 tracking issue carries these decisions.
13. **Consolidate first.** The run's first PR is behaviour-neutral: split
    `rules.rs` into a module per layer and split the RU address regex
    (9.39 of 10 MB). Fixtures and holdout results must be identical.
14. **Synthetic hunting only reproduces.** The cell saturation rule and the
    dry probe round leave the Done condition; synthetic documents are
    written only to reproduce real-set misses as fixtures.

## Round 3 (all agreed as recommended)

15. **Order of run 4.** (1) Land #751, #752 and Windows' two reviewed
    branches (part of #377, part of #549; done work, opened before the
    split so it doesn't conflict with them). (2) The consolidation PR and
    `scripts/hub-gate.sh`. (3) The two maintainer decisions, both misses:
    the MRZ document number is IDENTITY (#495), a US SSN is IDENTITY
    (#381). (4) Collect and gold the real set, in parallel with (2) and
    (3). (5) Triage, then fix misses and false-hit classes over item 9's
    line.
16. **The real set's gold.** A Sonnet sub-agent collects round 3 now; an
    Opus annotator writes the gold; Codex confirms it after its weekly
    reset (2026-11-10 09:09 local), since a second provider misses
    different things and this set is the finish line.
17. **The spike runs on `wsl`** (RTX 2070 SUPER, CUDA) as an Opus session
    now: the holdout first, the real set once it exists, then a short speed
    run on the Mac.
18. **rc.1** when the targets are met: the hub posts the scores and the
    maintainer tags it. The backlog and the spike never delay it.
19. **Hub limits.** At most 2 builders and 3 open PRs; an Opus pre-gate
    review (Codex once it has usage); the hub enqueues after
    `hub-gate.sh` passes and re-enqueues infra drops unchanged itself.
20. **Closing.** ADR 0108, ROADMAP items 3 and 4, the glossary's Real set,
    this record in one docs PR; #471 closes with a summary and a run 4
    tracking issue opens. The maintainer starts the Windows hub from a
    prompt after the docs PR merges; the spike's brief is written here.
