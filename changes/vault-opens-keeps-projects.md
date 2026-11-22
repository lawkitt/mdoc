- **The Vault's store.** A new core crate, `mdoc-vault`, keeps Projects and
  Documents as one encrypted file each, in the saved cases' format, with a
  rebuildable list of them and one write at a time across processes. Nothing
  uses it yet; later Vault & Projects items move the apps onto it
  ([ADR 0125](docs/adr/0125-unified-encrypted-vault-and-projects.md),
  [#1091](https://github.com/lawkitt/mdoc/issues/1091)).
