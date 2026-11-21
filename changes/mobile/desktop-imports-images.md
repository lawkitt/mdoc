- **A photo's location stays out.** A JPEG imported for text recognition
  no longer carries its EXIF metadata (GPS position, camera, dates) into
  the PDF the core builds, which is turned upright by its orientation and
  no longer written to a temporary file (refs #1021).
