- **The name detector needs about half the memory.** Setup re-saves its
  encoder with the weights in a file beside it, which ONNX Runtime loads
  without keeping a second copy: finding names in a 10,000-character
  document peaked at about 2.5 GB on a Mac and now at about 1.2 GB (the
  optional FP32 model drops similarly). An existing install converts once,
  without a new download (#905).
