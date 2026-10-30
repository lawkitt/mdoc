- **Windows checkouts on a Dev Drive.** On the Windows PC the root checkout,
  lanes `windows-lane-1` to `windows-lane-4` and local CI's home (`E:\ci`) are
  on `E:`, a ReFS Dev Drive, instead of `C:` and `D:`. Re-running
  `scripts/local-ci-setup.ps1` with a new `-Root` now also ends the results
  server left over from the old home, which otherwise kept answering on the
  same port (fleet ADR 0019).
