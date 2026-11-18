- **OCR setup works while GitHub's ONNX Runtime releases are unavailable.**
  When the ONNX Runtime 1.27.0 download from GitHub fails, setup takes the
  same version from Microsoft's NuGet package, checked against its own
  pinned digest; a damaged download still stops setup
  ([#925](https://github.com/lawkitt/mdoc/issues/925)).
