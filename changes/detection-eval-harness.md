- **Detection evaluation harness.** `tools/detection-eval/` scores candidate
  detection models and union combos through the app's own correction
  pipeline (`just recognize --scoreboard --snapshot`), exports ONNX builds and
  times them on the CPU, for choosing the detection options (#903). Nothing
  in the app changes.
