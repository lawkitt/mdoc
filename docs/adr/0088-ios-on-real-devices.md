# iOS on real devices

Status: Accepted, 2026-10-31. Amended 2026-10-31: the floor is 8 GB, measured.

## Problem

The first run on real iPhones (an iPhone 17 Pro and an iPhone 12 Pro) after
the UI polish ([ADR 0087](0087-ios-ui-polish.md)) crashed on every long job:
the background task's handler tripped Swift's main-actor check on the
scheduler's queue (fixed in lawkitt/mdoc#290). The Simulator never runs that
task, so no test reached it. The same run found a model download that waits
forever on unlimited 5G, a rules-only tier that finds emails and IBANs but no
names, and wording that reads an interrupted import as missing OCR.
Findings: [device grill](../design/ios-device-grill-2026-10-31.md).

## Decision

- **No rules-only tier.** A phone below the detector floor has no "rules
  instead of the model" scan. Names are what mdoc exists to hide; a scan
  that finds other details but no names gives a false sense of safety,
  labelled or not. This replaces ADR 0085's below-floor bullet.
- **Below the floor, mdoc still reads, recognizes and lets you mark.** Such
  a phone installs (no `UIRequiredDeviceCapabilities` gate; the App Store
  cannot gate on RAM), imports, recognizes text and shows the Markdown, and
  selecting text still marks, replaces, shares and restores. Where Find
  names would be, the document says the phone cannot find names, why
  (the name detector's memory floor and the phones that meet it) and that
  selecting text marks a name; Settings says the same. There is no detector
  setup card on such a phone.
- **The floor is 8 GB, measured.** The floor is the lowest RAM class that
  meets every one of ADR 0085's detector budgets at desktop quality. On
  GLiNER2 FP16 the iPhone 12 Pro (6 GB) met memory, load and heat but its
  20-page scan ran past the two-minute deadline (budget 60 s), while the
  iPhone 17 Pro met all four
  ([evidence](../evidence/mobile/2026-10-31/README.md)). INT8 was not
  tried, by the maintainer's choice: it would need 2 to 2.3 times FP16's
  speed on the 12 Pro. So the floor is 8 GB (the iPhone 15 Pro and every
  iPhone from the 16 on); `DETECTOR_FLOOR_BYTES` (7 GiB, between the two
  classes) and the below-floor wording follow it.
- **"On cellular, mdoc asks first" means any cellular interface**, whatever
  iOS's expensive flag says: unlimited 5G with "Allow More Data" is not
  marked expensive. The Wi-Fi session never uses cellular; only a yes to the
  question starts the cellular one.
- **A download held for Wi-Fi says so**: "Waiting for Wi-Fi", with "Use
  cellular", which asks the question.
- **An interrupted import says so**: "Import not finished" and Finish import
  for documents that need no recognition; scans and photos keep the
  recognition wording.
- **The + menu reads Paste, Files, Photos, Scan**: the camera last, away
  from the button.
- **Device UI tests catch what the Simulator cannot.** `just
  ios-device-test <udid>` copies the fixtures into mdoc's container on a
  connected iPhone with `devicectl` (the app ships none) and runs the review
  flow and a long-jobs test: .docx and text-PDF import, a scan with a real
  text-recognition download on Wi-Fi, Find names, and leaving the app during
  a job, all through the real background task. A phone below the floor runs
  a below-floor test instead: the same download and scan, the note where
  Find names would be and in Settings, marking by selection, and a .docx
  import that is whole after leaving the app. They run by hand before
  every TestFlight build after the first.
- **The first TestFlight build waits** for the measured floor and the device
  checklist (P17) on the phones that run the detector.

## Invariants

- ADR 0085's rules hold: no network except model downloads, read-only
  source text, nothing in backups, every string in the String Catalog,
  VoiceOver and Dynamic Type on every new control.
- Handlers the system calls on its own queues carry no actor isolation.

## Consequences

- The phone and IBAN rules drafted for the rules-only tier
  (lawkitt/mdoc#288) are withdrawn with ROADMAP item 4; the structured
  rules keep running beside the model as before.
- Platform acceptance gains P17, the device checklist.
- The rules-only setup card, its wording and its survey screens go.
