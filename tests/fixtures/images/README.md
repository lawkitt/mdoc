# Image fixtures

Synthetic images, not user photos.

- `gps-rotated.jpg`: a 64×48 flat green JPEG with an EXIF block holding a
  fictional camera make (`FictionalCam`), a GPS position (50° 27′ N) and
  orientation 6. Written by the ignored test `write_image_fixtures` in
  `crates/mdoc-import/src/image_pdf/tests.rs`
  (`cargo nextest run -p mdoc-import write_image_fixtures --run-ignored only`).
  The phones' intake test imports it.

The image builder's own tests make their images in code.
