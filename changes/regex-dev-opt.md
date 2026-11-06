- **The recognition fixture test fits its time limit again.** Dev and test
  builds optimize the regex crates, where the rule pack spent most of its
  time compiling and matching patterns unoptimized: the test over every
  recognition fixture takes about 3 s instead of 14 on the M4, so a full
  `just test` on the PC queue runner no longer kills it at 2 minutes and
  drops the queue entry (fixes #655).
