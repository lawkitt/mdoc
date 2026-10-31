# iOS on real devices: findings and next steps (grill, 2026-10-31)

The first run of the iOS app on two real iPhones after the UI polish
([ADR 0087](../adr/0087-ios-ui-polish.md)): the iPhone 17 Pro (iOS 27.0.1,
detector tier) and the iPhone 12 Pro (iOS 27.2, rules only). The user saw
the app crash "in many cases: downloading a model, opening a .docx or PDF".

## Findings

1. **Every long job crashed the app** (fixed in lawkitt/mdoc#290). All nine
   crash reports from both phones are one `EXC_BREAKPOINT` in the
   `BGTaskScheduler` launch handler made inside the `@MainActor`
   `ContinuedProcessing.begin`: Swift 6's runtime isolation check stops the
   app when the scheduler calls it on its own queue. Model downloads,
   imports, text recognition and Find names all start that background task.
   The Simulator refuses `BGContinuedProcessingTask`, so no test reached it.
   No memory (jetsam) report names mdoc. After the fix, on the 17 Pro,
   recognizing a document whose import had crashed and importing a .docx
   from Files both finish, with no new crash report.
2. **A model download on 5G waits forever without saying so.** With an
   unlimited plan and "Allow More Data on 5G", iOS reports 5G as not
   expensive, so mdoc skips its cellular question and starts the Wi-Fi
   session, which allows no cellular at all (`allowsCellularAccess = false`).
   The row says "Downloading" at 0 % until Wi-Fi returns.
3. **Find names on a detector-tier phone without models offers only the
   649.5 MB download.** Not now hides the card and leaves no way to scan.
4. **An interrupted import reads as missing OCR.** A .docx whose import was
   stopped shows "Text not recognized yet" on Home and Recognize text in the
   document, though it needs no recognition.
5. **The + menu opens Scan under the button**, so a double tap opens the
   camera (and the camera permission prompt).
6. **Not checked on a device yet:** a model download to the end, Replace
   from a selection, Share, the Restore extension, and anything on the
   iPhone 12 Pro (iPhone Mirroring shows one phone, and cannot open the
   edit menu, which needs a long press).

## Decisions

### Round 1 (all recommendations accepted)

1. **#290 merges first**, as soon as its review and CI pass, before the last
   polish PR: every device build crashes until it lands.
2. **"On cellular, mdoc asks first" means any cellular interface**, whatever
   iOS's expensive flag says (unlimited 5G with "Allow More Data" is not
   marked expensive). A model download is hundreds of megabytes, and the
   setup card already promises this. Invariant: the Wi-Fi session never
   uses cellular; only a yes to the question starts the cellular session.
3. **A download held for Wi-Fi says so**: "Waiting for Wi-Fi" with a "Use
   cellular" button that asks the cellular question (URLSession reports a
   task waiting for connectivity).
4. ~~Not now on a detector-tier phone without models runs a rules-only
   scan.~~ Reversed by A below: setup comes first, as today.
5. **An interrupted import says so**: "Import not finished" on Home and a
   "Finish import" button for documents that need no recognition; scans and
   photos keep the recognition wording.
6. **The + menu reads Paste, Files, Photos, Scan**: the camera last, away
   from the button.
7. **Device-only bugs are caught by UI tests run on a connected iPhone**:
   fixtures inside the test bundle, a `just ios-device-test <udid>` recipe,
   run by hand before each TestFlight build, and an acceptance row whose
   steps are the manual checklist.
8. **The flows not yet checked on a device get a checklist now**, run by the
   user on both phones with the #290 build; the device tests come later.

### Revised: the rules-only tier (maintainer's challenge)

The maintainer asked whether a rules-only scan is a good product decision:
mdoc should give desktop-quality detection on the phones that can run the
models, not heuristics for weak devices.

A. **The rules-only tier is dropped.** A phone below the floor does not run
   mdoc and says so plainly; there is no "rules instead of the model" mode.
   Names are what mdoc exists to hide; a phone that finds emails and IBANs
   but no names gives a false sense of safety, labelled or not. The rules
   themselves stay: they run beside the model on every device.
   The iPhone 12 Pro becomes the device that checks the refusal.
B. **The floor is measured, not assumed.** The iPhone 12 Pro is measured
   against ADR 0085's budgets (peak memory 2 GiB or less, cold load 10 s or
   less, 20 pages in 60 s or less, no serious throttling) with GLiNER2
   FP16, then INT8 if FP16 misses; INT8 counts only if it matches FP16 on
   the qualification fixtures. The floor is the lowest RAM class that meets
   every budget at desktop quality: 6 GB if the 12 Pro passes, else 8 GB
   (today's `DETECTOR_FLOOR_BYTES`, 7 GiB, between the classes).

Facts for what follows: the App Store cannot gate installs on RAM. The
nearest key, `iphone-performance-gaming-tier` in `UIRequiredDeviceCapabilities`,
admits about iPhone 15 Pro and later (the 8 GB class on iPhone; M1 and later
on iPad), and has been reported to switch on Game Mode.

### Round 3

C. **A phone below the floor installs and is told in the app**, whatever
   the floor: no `UIRequiredDeviceCapabilities` gate. It still imports,
   recognizes text (PDFs, scans, photos) and shows the Markdown; only Find
   names needs the detector.
D. **The phone and IBAN rules grill (#288) and ROADMAP item 4 are
   dropped**: they were for the rules-only tier (lawkitt/mdoc#288 closed).
E. **Three sessions after #290 and #287 merge**: "iOS device fixes" (the
   cellular question on any cellular interface, "Waiting for Wi-Fi", the
   import wording, the menu order, and removing the rules-only tier),
   "iOS device UI tests", and "12 Pro detector measurement" (sets the
   floor).
F. **The device UI tests get their fixtures through `devicectl`**, copied
   into mdoc's container on the phone by the recipe; the app ships none.
G. **They cover the review flow and long jobs**: .docx and text-PDF import,
   a scan with a real text-recognition download on Wi-Fi, Find names, and
   leaving the app during a job.
H. **[ADR 0088](../adr/0088-ios-on-real-devices.md) "iOS on real devices"** holds these decisions; ADR 0085's
   floor and tier bullets point to it; the checklist becomes acceptance row
   P17.
I. **The first TestFlight build waits for the measured floor and the
   checklist** on the phones that run mdoc; the device UI tests gate every
   build after it.

### Round 4

J. **Below the floor, manual marking stays**: selecting text marks,
   replaces, shares and restores; only finding names needs the detector.
K. **The phone says so where Find names would be**, and in Settings: it
   cannot find names, the memory floor and the phones that meet it, and
   that selecting text marks a name. No detector setup card.

The frontier is empty.

## Device checklist

Run on both phones with a build of `main` that has #290 and the polish
stack, starting from a fresh install (delete mdoc first). After the run,
the crash reports are pulled with `xcrun devicectl device copy from
--domain-type systemCrashLogs`; a pass leaves no new `mdoc-*.ips`.

1. Open mdoc: Home shows the setup card (12 Pro: "Set up text
   recognition", rules only; 17 Pro: name finding).
2. On Wi-Fi, download the models (17 Pro 649.5 MB, 12 Pro 18 MB). Leave
   the app while it runs and come back: the system shows the job, and it
   finishes.
3. + → Files: a .docx opens with its text.
4. + → Files: a text PDF opens with its text; a scanned PDF runs text
   recognition.
5. + → Scan: a paper page runs text recognition.
6. + → Paste: pasted text, a title and a case, then Add.
7. Find names on the .docx: the 17 Pro underlines names; the 12 Pro
   underlines email addresses and labelled numbers.
8. Long-press a name → Replace… → Replace.
9. Share (one tap) → Copy; long-press Share → a Markdown file.
10. In Notes, a reply with an alias → Share → Restore with mdoc: the name
    is back. Paste reply in the document: the reply is styled.
11. Lock the phone during Find names on a long document: on return it has
    finished, or offers to resume.
