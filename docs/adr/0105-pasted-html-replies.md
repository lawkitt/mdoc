# Pasted replies read their HTML

Status: Accepted, 2026-11-07. Applies to the mobile apps' Paste reply and
Restore with mdoc. [ADR 0078](0078-restore-in-place.md)'s plain text box on the
desktop stays as it is.

## Problem

ADR 0056 captured ChatGPT's Copy as Markdown, and Restore renders Markdown.
The ChatGPT iOS app's copy holds none. One capture on 2026-11-07
(`crates/mdoc-markdown/fixtures/from_html/`) carries HTML with a real table,
headings and lists, the same as RTF, and a flattened plain text: headings as
plain lines, table cells split by tabs, lists marked `*`. mdoc read the plain
text, so a reply's tables and headings were gone before anything rendered them.

## Decision

- **The core decides.** `pasted_markdown` in `mdoc-markdown` takes a copy's
  HTML and plain text and returns the text to use; the apps only read the
  pasteboard or the share (ADR 0104).
- **The HTML is used only when it adds something.** Plain text showing a
  Markdown heading, table separator or bold is the source and stays byte for
  byte, as with an AI app's Markdown Copy or a Markdown editor. A list doesn't
  count, since the flattened text writes `*` too. HTML that is only paragraphs
  leaves the plain text, or its `|` and `#` would be escaped.
- **Aliases pass untouched.** `_` is never escaped, so Restore finds every
  alias. Other characters Markdown would read are escaped, and a paragraph that
  starts like a list or heading stays a paragraph.
- **No text is lost to markup.** An unknown tag keeps its text; `head`,
  `style` and `script` go. A link keeps its target only for `http`, `https`
  and `mailto`.
- **Every way in.** The Paste button, the text box's own Paste and the share
  extension read a copy the same way.
- **`html5ever`'s tokenizer**, not a converter crate. It gives correct tags and
  character references for broken input; the conversion is ours because of the
  alias rule. It adds 11 crates under MIT or Apache-2.0.

## Consequences

- The box shows the Markdown, so what Restore gets can be read before it runs.
- Pasting a document (+ › Paste text) still takes plain text.
- Android gets the same through `mdoc-ffi` when it next moves.
- ChatGPT can change what it copies; the fixture records one capture.
