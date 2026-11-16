- **The name detector needs about half the memory.** The core re-saves its
  encoder with the weights in a file beside it, which ONNX Runtime loads
  without keeping a second copy. An installed detector converts once,
  without a new download (#905).
