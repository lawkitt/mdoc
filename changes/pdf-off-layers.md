- **Text in a hidden PDF layer stays out of the import.** A PDF can keep a
  layer turned off, so no reader shows its text. That text, including text
  in forms, annotations and form fields on such a layer, no longer reaches
  the Markdown; the import note counts it with the other hidden passages
  (fixes [#1004](https://github.com/lawkitt/mdoc/issues/1004)).
