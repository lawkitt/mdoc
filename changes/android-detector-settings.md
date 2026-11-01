- **Android spike: detector settings.** The Android feasibility spike also
  times the name detector's window at 2, 4, 6 and 8 threads, on ONNX
  Runtime's CPU provider and on XNNPACK, for the floor decision
  ([ADR 0089](docs/adr/0089-android-app.md)). The detector's thread count
  can be set with `MDOC_DETECT_THREADS`; the apps keep 4.
