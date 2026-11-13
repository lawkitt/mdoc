- **CI: the iOS UI tests run in two shards.** `ci/ios (ui 1)` and
  `ci/ios (ui 2)` replace `ci/ios (ui)`, each running half of the UI test
  classes, as `scripts/ios-ui-shards.sh` lists them by their time on the
  hosted runner; a new UI test class fails `lint (ubuntu)` until it is in
  exactly one shard, and a failing `ci/ios` names the part that failed
  (ADR 0112).
