# Phone detector floor and scan time

Status: Accepted, 2026-11-16.

## Problem

ADR 0085 offered the detector only on phones that scan 20 pages within
60 s with no serious throttling, which set an 8 GB floor. That floor came
from speed, not memory: the 6 GB iPhone 12 Pro fit in memory but needed
about 123 s, and no Android phone met 60 s. Since #905 the detector peaks
at about 1.1 GB in the app. Owners of older phones would rather wait.

## Decision

1. **Floor: 4 GB of physical RAM** (`DETECTOR_FLOOR_BYTES`, 3.25 GiB,
   between the 3 GB and 4 GB classes), **plus a free-memory check at scan
   start**: about 1.3 GB free (`DETECTOR_FREE_BYTES`), or the scan says so
   instead of risking a kill. Every iOS 26 iPhone but the 3 GB SE 2nd
   generation qualifies; 3 GB waits for a measured 3 GB phone.
2. **No fixed time limit on phones.** A scan runs to the end with progress,
   the time it has left and Cancel; it stops only after 60 s without
   progress. Desktop keeps ADR 0002's two-minute deadline.
3. **Tell users how long it takes.** Right after the detector installs, the
   phone times one short run and keeps it. A scan expected to take more
   than about a minute shows "About N min on this phone" first; Settings
   shows a figure per 10 pages. Speed is kept per character, since a
   window holds whole paragraphs of any length.
4. **A speed ceiling.** A phone whose timed run exceeds 10 minutes per
   10,000 characters counts as below the floor and its model is removed.
5. **Heat.** No throttling budget for offering the detector. A scan pauses
   at the critical thermal state ("Paused to cool down") and goes on once
   the phone cools to serious or below; paused time counts toward no limit.

This supersedes ADR 0085's 8 GB floor and its 60 s scan and thermal
budgets; its 2 GiB memory and 10 s load budgets stand.

## Consequences

- Long scans run for minutes, so they rely on the background continuation
  phones already have (`BGContinuedProcessingTask`, a foreground service).
- The iOS release gate's iPhone 12 Pro now runs at the detector tier.
- A 4 GB phone is unmeasured until a device farm or borrowed phone runs it.

References: [issue #907](https://github.com/lawkitt/mdoc/issues/907); rules
in [docs/specs/mobile.md](../specs/mobile.md).
