- **The detection gate says how close it is to its budget.** A shard warns
  past 80% of its 5-minute model budget (on 3 shards each took 84%, so the
  queue keeps 4), manual runs default to 4 shards instead of 2, which went
  over the budget, and a stale snapshot's message
  shows both scores to five decimals and how far they moved
  ([ADR 0100](docs/adr/0100-ci-reliability-after-the-cutover.md)).
