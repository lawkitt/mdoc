- **The name detector needs about half the memory.** Setup re-saves its
  encoder with the weights in a file beside it, which ONNX Runtime loads
  without keeping a second copy: a scan with the default model peaked at
  about 2.45 GB and now at about 1.2 GB (FP32: 3.6 to 1.35 GB). An existing
  install converts once, without a new download (#905).
