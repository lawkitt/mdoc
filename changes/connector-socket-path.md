- **The AI connector starts from a profile with a long path.** On macOS
  and Linux, when the connector's usual socket path is too long for the
  system, mdoc and `mdoc-mcp` use a short owner-only folder of the
  profile's own; if even that can't fit, Settings says the path is too long
  in plain words instead of "SUN_LEN" and a retry (fixes #1311).
