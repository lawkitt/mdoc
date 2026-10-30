- **The core builds for iOS.** `mdoc-import`, `mdoc-detect` and `mdoc-store`
  build for iPhone and the Simulator, and the new `crates/mdoc-ffi` exposes
  them to Swift through UniFFI; `just ios-core` assembles
  `MdocCore.xcframework` with ONNX Runtime linked in and PDFium as an embedded
  framework. A throwaway harness measured GLiNER2 and PP-OCR against the
  mobile detector budgets; phone rows await the maintainer
  ([ADR 0085](docs/adr/0085-mobile-apps.md),
  [report](docs/evidence/mobile/2026-10-31/README.md)). Desktop is unchanged.
