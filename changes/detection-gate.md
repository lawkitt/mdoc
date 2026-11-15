- **Detection gate.** Recognition fixtures with `⟦CATEGORY|text⟧` gold
  and the model's raw output as snapshots
  ([tests/fixtures/recognition](tests/fixtures/recognition/README.md)),
  `just recognize` (`--gold`, `--snapshot`, `--scoreboard`), a snapshot
  test on every OS and a model run in `local/macos`, so a change that
  breaks a fixture fails CI; #301's spans are known-flaw fixtures
  ([ADR 0093](https://github.com/lawkitt/mdoc/blob/0aaf8f706e754d8b4c9ad905305b7bac194218a1/docs/adr/0093-flaw-hunt-loop.md)). The scan is split into a
  model stage and a correction stage with no change in what it proposes.
