- **A document that crashes the converter no longer closes mdoc.** Word,
  Excel, PowerPoint, OpenDocument, RTF, EPUB and CSV files now convert in a
  separate worker process, as the DOCX preview already did. If the
  converter crashes, runs out of memory or takes longer than two minutes,
  only the worker stops: the tab shows an import error and your other
  documents stay open. Cancelling an import stops the worker at once. PDFs
  are unchanged ([#925](https://github.com/lawkitt/mdoc/issues/925)).
