- **Text recognition gives memory back after each page.** mdoc no longer
  keeps the working memory of the biggest scanned page it has read until it
  quits: after recognizing many pages at the default 150 dpi, it holds about
  0.25 GiB instead of 1.2 GiB on a Mac, with the same text and about the
  same speed ([#925](https://github.com/lawkitt/mdoc/issues/925)).
