- **CI: the three iOS UI shards balance on `macos-26` times.**
  `apps/ios/ui-class-times.tsv` holds the class times measured on `macos-26`,
  where the shards run, instead of the 2–3 times slower `xcode-27` ones, and
  the nightly refreshes it from the tests pin's UI parts only.
