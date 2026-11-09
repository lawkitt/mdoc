- **Email addresses with escaped underscores.** An address whose
  underscores are Markdown-escaped, as a Word import writes them
  (`snake\_case@…`, `\_roman@…`), or read as emphasis or strikethrough
  inside it (`__init__@…`) is proposed whole, in code too, so no part of
  it stays visible; emphasis around an address stays (fixes #746).
