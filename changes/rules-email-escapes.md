- **Email addresses with escaped underscores.** An address whose
  underscores are Markdown-escaped, as a Word import writes them
  (`snake\_case@…`, `\_roman@…`), or read as emphasis inside it
  (`__init__@…`) is proposed whole, so no part of it stays visible;
  emphasis around an address and code stay as they were (fixes #746).
