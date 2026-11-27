- **Model downloads resume after a network error.** A download that fails
  partway keeps what it has and goes on from there, on an automatic retry
  of a dropped connection or the next Download or Repair; the whole file's
  digest is still checked. A slow connection no longer fails a large file
  after 10 minutes, and a stalled one fails within a minute or so rather
  than holding setup (fixes #1288).
