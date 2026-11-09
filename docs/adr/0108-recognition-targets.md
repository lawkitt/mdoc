# Recognition targets

Status: Accepted, 2026-11-09. Amends [ADR 0093](0093-flaw-hunt-loop.md)
(stopping, the holdout, hunting) and the run decisions on
[#471](https://github.com/lawkitt/mdoc/issues/471). Record:
[run 3 grill](../design/recognition-run3-grill-2026-11-09.md).

## Problem

Run 3 merged about 113 recognition PRs in a week and moved holdout recall
from 91.1% to 97.6% (precision 92.1% to 94.0%), yet 52 of 99 flaw classes
are still open and only about 9 closed in six days. Most fixes cover part
of a class, each probe round reopens classes with new shapes, and many
false-hit classes are open-ended lists. ADR 0093's stop (every class
closed or `model-limit`, every cell saturated, a dry probe round) cannot be
reached. Nothing scores mdoc on real documents, run 3 never ran the
holdout, and the two-hub split left the Mac hub only gating work the merge
queue tests anyway.

## Decision

- **Done is a set of targets.** ROADMAP 0.4.0 items 3 and 4 are done when,
  on the real set:
  - recall is at least 95% overall and at least 90% in each leaking
    category: PERSON, ADDRESS, IDENTITY, TAX, BANK, PHONE, EMAIL, date of
    birth and CASE;
  - precision is at least 85%;
  and the holdout stays at or above 97.6% recall and 94.0% precision, and
  both demos stay clean. The numbers are revisited once, when the real
  set's first score is known.
- **Misses block; false hits by volume.** A miss class blocks 0.4.0. A
  false-hit class is fixed only when it appears in at least 3 real-set
  documents or makes up at least 5% of the real set's false hits; the rest
  go to the "Recognition backlog" milestone and return when they cross
  that line.
- **The real set.** About 25 EN and 25 RU real public documents from a
  fresh probe round (round 3; same sources and kinds as ADR 0093), kept in
  `lawkitt/mdoc-probes`. Gold is written once by an Opus annotator against
  GUIDE.md and confirmed by Codex, then frozen. No fixer's brief sees the
  documents or their gold; only aggregates are posted. Probe rounds 1 and 2
  stay hunting ground, since fixes were made from them.
- **Hunting only reproduces.** The cell saturation rule and the dry probe
  round leave the stop condition. Synthetic documents are written to
  reproduce real-set or probe misses as fixtures, not to find new classes.
- **The gate is a script.** `scripts/hub-gate.sh <PR>` merges main, every
  PR ahead in the merge queue and the candidate, runs
  `cargo nextest run --locked -p mdoc-detect -p mdoc-pii`, and runs the
  holdout when its private path is set. The hub enqueues only after it
  passes and logs the holdout aggregates; a holdout fall stops fixing and
  goes to the maintainer.
- **One hub.** A single Windows hub builds, gates and enqueues: at most 2
  builders and 3 open PRs, an Opus review before each PR opens (Codex when
  it has usage), never a direct merge past the queue. A PR dropped by an
  infrastructure failure (#724, #737) is re-enqueued unchanged by the hub.
  The Mac takes part only for a snapshot that must be recorded locally.
- **Consolidation first.** Run 4's first PR is behaviour-neutral: `rules.rs`
  split into a module per layer and the RU address regex (9.39 of 10 MB)
  split, with identical fixture and holdout results.
- **A model spike, beside the run.** A one-day session scores a GLiNER2
  fine-tuned on freshly generated synthetic documents (never fixtures) and
  one or two off-the-shelf multilingual PII models at most twice the
  current size: alone and under the pipeline, on the holdout and the real
  set, with speed on the Mac. Nothing merges; it ends in a report and a
  maintainer decision for 0.5. It never delays rc.1.
- **rc.1.** When the targets are met the hub posts the scores and the
  maintainer tags rc.1; the backlog does not block it.

## Consequences

- The run can end, and its progress is a number on real text rather than a
  count of open issues.
- About 36 false-hit classes leave 0.4.0 after the first real-set score;
  users see more of those false hits until the backlog is worked.
- The real set is small; one document can move a category's recall by
  several points, which is why the targets are revisited once.
- ADR 0092's pipeline and autonomy rules are unchanged.

Terms: [glossary](../../CONTEXT.md).
