- **A stale snapshot is red on the PR, not in the merge queue.** A PR that
  changes a recognition fixture or the recognition code (`mdoc-detect`,
  `mdoc-pii`) runs the detection gate on its head (`detection-head /
  ci/detection`, 4 shards), and its `merge-gate` waits for it and names
  `just snapshots-fetch <PR>` when it fails; a core or desktop change alone
  still runs it only in the queue. Lanes no longer dispatch the gate by hand
  ([ADR 0100](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0100-ci-reliability-after-the-cutover.md)).
