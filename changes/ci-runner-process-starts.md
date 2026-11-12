- **CI: the PC's queue runner starts far fewer processes.** Reading the area
  table no longer starts one per line, and the runner's loop takes the time,
  a commit's short name and its done list from bash itself. On Windows, where
  Git Bash needs about 0.1 s for each, that cuts the script suites' process
  starts by 40 to 60 percent and the idle runner's own load.
