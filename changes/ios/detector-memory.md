- **The name detector needs about half the memory.** The core re-saves its
  encoder with the weights in a file beside it, which ONNX Runtime loads
  without keeping a second copy: finding names in a 10,000-character
  document peaked at about 2.1 GB on an iPhone 17 Pro and now at about
  1.1 GB. An installed detector converts once, without a new download
  (#905).
