- **The detection gate runs on 3 shards and says how close it is.** The
  merge queue and the nightly split the recognition fixtures over 3 hosted
  macOS jobs instead of 4, freeing a runner per queue group; a shard warns
  past 80% of its 5-minute model budget, and a stale snapshot's message
  shows both scores to five decimals and how far they moved
  ([ADR 0100](docs/adr/0100-ci-reliability-after-the-cutover.md)).
