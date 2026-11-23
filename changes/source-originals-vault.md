- **Source originals in the Vault.** `mdoc-vault` keeps each imported
  file as it was, locked ones still locked, sealed under its own key in
  1 MiB chunks that are written and read without holding the file in
  memory. It records the file's SHA-256 so a second import of the same
  file can be found, and writes a viewing copy into a folder the caller
  names. Nothing uses it yet
  ([ADR 0125](docs/adr/0125-unified-encrypted-vault-and-projects.md),
  [#1091](https://github.com/lawkitt/mdoc/issues/1091)).
