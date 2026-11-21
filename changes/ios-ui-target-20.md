- **The iOS UI shard target is 20 minutes.** The nightly's CI targets
  report judges the slowest UI shard's job against a 20-minute median
  instead of 15: on `macos-26` a balanced shard's tests take about 17
  minutes, which the maintainer accepted over another test sweep
  ([#1041](https://github.com/lawkitt/mdoc/issues/1041)). The core and
  iOS UI targets count from #1053's merge.
