//! GUI-free Markdown recognition, shared by mdoc's desktop editor and its
//! mobile apps (ADR 0085): what counts as a construct and which source bytes
//! it covers, never how it looks.
//!
//! - [`spans`]: inline spans over source byte ranges — emphasis, strike,
//!   highlight, code, links, styled inline HTML, bare URLs — and which bytes
//!   are syntax markers, plus the hidden line prefixes.
//! - [`lines`]: one line's ATX heading, thematic break, blockquote, list or
//!   task prefix.
//! - [`tables`]: GFM table regions, cells and alignment.
//! - [`syntax`]: the grammar the reader shares with the editor: alerts,
//!   table styles, writing direction, links, highlight and styled tags.
//!
//! Offsets are UTF-8 byte offsets into the source; markers are ASCII.

pub mod lines;
pub mod spans;
pub mod syntax;
pub mod tables;
