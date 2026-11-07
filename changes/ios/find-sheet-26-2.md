- **Find no longer freezes the app on iOS 26.2.** Opening Find in document
  focused its search field while the sheet was still sliding up, which on
  iOS 26.2 kept the app busy and unresponsive; the field now takes focus
  once the sheet has finished opening, and asks again until it has it, so a
  slow slide-up no longer leaves Find without a cursor (fixes #634, #669).
