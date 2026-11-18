- **OCR and pseudonymization run on ONNX Runtime 1.32.0.** Setup downloads
  it from Microsoft's NuGet package (184 MB), checked against pinned
  digests, because GitHub no longer serves Microsoft's ONNX Runtime
  releases. A computer set up with 1.27.0 downloads the new runtime once,
  when OCR or pseudonymization setup runs again; its models stay
  ([#925](https://github.com/lawkitt/mdoc/issues/925)).
