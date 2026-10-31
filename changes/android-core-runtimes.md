- **Mobile core on Android.** The core opens the ONNX Runtime and PDFium
  that ship inside the Android app, reads the process's memory there for the
  feasibility spike, and leaves out the desktop's DOCX preview worker
  ([ADR 0089](docs/adr/0089-android-app.md)).
