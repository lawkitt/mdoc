# Mirage on candle as the vision engine

Status: Paused, 2026-11-17: no class passed #911's evaluation; resumes only
on a follow-up run that passes.

## Problem

OCR reads seals, signatures, QR codes and repeated headers as junk text, and
pictures and the PII in them never reach the review. PP-OCR has no region
classes. Mirage (Cactus Compute; Apache-2.0 weights; 51M parameters) detects
open-vocabulary objects with masks, but its only runtime, libcactus, is a
closed-source static library, and no reference implementation is public.

## Decision

- A vision layer beside OCR finds page artifacts; PP-OCR keeps reading all
  text, and Mirage never does mdoc's text OCR.
- Mirage runs from a Rust port on candle in its own repository,
  `lawkitt/mirage-rs` (Apache-2.0, attributed to Cactus), pinned by revision
  like the forks and loading the pinned upstream safetensors or our own
  quantization of them.
- Mirage is the one exception to inference through the app's one ONNX
  Runtime; the Paddle layout and seal models stay on it.
- libcactus is only the evaluation's reference and the port's parity
  oracle; it never ships.
- pdf-inspector gets a `RegionDetector` contract beside `OcrEngine`, which
  each engine implements; Mirage's boxes (0–1000 per image axis) become
  pixel quads at that boundary.
- The port comes in two stages: the `objects` and `text` sections first,
  then relations, reasoning, forms and keywords.

## Invariants

- The port is accepted only when it does at least as well as libcactus on
  the evaluation's bars, and against libcactus 4.1.1 on a fixed image set
  matches at least 90% of objects at box IoU 0.9 or more, at least 85% with
  the same name.
- The shipped precision is chosen per CPU platform from measurements.
- Which artifact classes ship, and from which engine, follows the
  evaluation's verdict.

## Consequences

- A second inference engine to build, pin, time on each platform and
  upgrade.
- The port is rebuilt from the weights, config and Cactus's published
  equations; a new Mirage release needs re-porting and a new parity run.

References: [grill record](https://github.com/lawkitt/mdoc/issues/911).
