- **A flaky review-flow UI test is quarantined.** `ReviewFlowUITests/testOpenReviewShareRestore`
  failed twice on unrelated changes, at waits on the share sheet and a
  toast; CI skips it until its waits are fixed (#638).
