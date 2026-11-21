# iOS tests on Xcode 26, builds on Xcode 27

Status: Accepted, 2026-11-21.

## Why

App Store uploads need the iOS 27 SDK from April 2027, and only GitHub's
preview `xcode-27` image has Xcode 27. On it a UI shard's tests took 21–34
minutes against about 10 on `macos-26`, and more tests flaked, so a core
change's queue time rose to a median of 51 minutes (#1041). So the
queue's and branches' iOS tests run on `macos-26` with Xcode 26.2 and the
iOS 26.2 Simulator, the app's deployment target being iOS 26.0, while the
Release build, the archive and a nightly test pass use Xcode 27.2 and iOS 27
on `xcode-27`. The cost: two Xcode pins, and the Mac (macOS 27, where Xcode
26.2's test runner hangs) reproduces a queue failure with Xcode 27.2 and the
iOS 26.2 Simulator runtime, the same iOS but a newer compiler. A test must
pass on iOS 26 and iOS 27.

## Rejected

- **Every iOS job on `xcode-27`**: two to three times slower tests and more
  flakes on a preview image.
- **Installing Xcode 27 on `macos-26` in each job**: a download of several
  GB per job, larger than the 10 GB cache.
- **Paid 5-core macOS runners**: about $1,500–2,000 a month on the Team plan,
  still capped at 5 macOS jobs.

## Revisit when

`xcode-27` leaves preview and its tests run within 20% of `macos-26`'s, or
the hosted images drop Xcode 26.
