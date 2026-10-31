- **iOS: long jobs no longer crash the app on a device.** Downloading a
  model, importing a document or finding names starts a background task,
  whose handlers stopped the app on a real iPhone (Swift's main-actor
  check on the scheduler's queue).
