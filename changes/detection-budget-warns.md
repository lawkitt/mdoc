- **Detection's model budget warns in the queue.** The merge queue and
  recognition-area PR heads run the detection gate in 4 shards instead of
  12, and a shard past its 5-minute model budget warns there instead of
  failing; the nightly still runs 12 and fails on it (#1041).
