# Mirage on candle as the vision engine

Status: Paused, 2026-11-17: no class passed #911's evaluation; resumes only
on a follow-up run that passes.

## Why

OCR reads seals, signatures, QR codes and repeated headers as junk text, and
pictures and the PII in them never reach the review; PP-OCR has no region
classes. Mirage (Cactus Compute, Apache-2.0 weights, 51M parameters) detects
open-vocabulary objects with masks, but its only runtime, libcactus, is a
closed-source static library. So a vision layer beside OCR finds page
artifacts while PP-OCR keeps reading all text, and Mirage runs from mdoc's
own Rust port on candle (`lawkitt/mirage-rs`), pinned by revision like the
forks. It is the one exception to inference through the app's ONNX Runtime,
at the cost of a second engine to pin, time on each platform and re-port for
each Mirage release. The port is accepted only when it matches libcactus
4.1.1 on at least 90% of objects at box IoU 0.9, and 85% with the same name.

## Rejected

- **Shipping libcactus**: closed source; it stays the parity oracle.
- **Mirage for text OCR**: PP-OCR stays the reader.

## Revisit when

A follow-up evaluation passes for an artifact class, or Cactus publishes a
reference implementation.
