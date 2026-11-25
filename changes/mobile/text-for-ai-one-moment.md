- **Copy for AI checks a document and its Project's names at one moment.**
  The outbound check reads both under the Vault's lock. A write that lands
  between the two reads, from another window or app, can no longer let the
  text from before a name was taken out of it leave with that name as
  written ([#1198](https://github.com/lawkitt/mdoc/issues/1198)).
