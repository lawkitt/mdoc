//! mdoc's Markdown editing rules without a GUI (ADR 0109).
//!
//! The desktop editor (`mdoc-editor`, GPUI) and the phones (through
//! `mdoc-ffi`) call these rules, so each one exists once (ADR 0104). The rules
//! work on the source text and UTF-8 byte offsets into it, and return the
//! edit to apply (a replaced range and its text) with where the caret or
//! selection lands; the caller applies it and records it in its history.
//!
//! Rules that need the document's block structure (code fences, display
//! math, tables) read it through [`Blocks`]: desktop lends its cached scan,
//! a stateless caller builds a [`BlockScan`] from the source.

use std::ops::Range;

pub mod blocks;
pub mod delete;
pub mod enter;
pub mod format;
pub mod history;
pub mod markdown;
pub mod paste;
pub mod reveal;
pub mod search;
pub mod spell_exclusions;
pub mod tables;
pub mod text;
pub mod transactions;
pub mod turn_into;
pub mod words;

pub use blocks::{BlockScan, Blocks};

/// Spaces one indent level adds (Tab on a list or quote item).
pub const TAB_INDENT: usize = 4;

/// One replacement in the source: the byte `range` it replaces and its new
/// `text`. A list of edits is sorted and non-overlapping, in the offsets of
/// the source before any of them applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

/// What a command does: its edits and the selection after them (in the
/// offsets of the edited source; an empty range is a caret).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditResult {
    pub edits: Vec<Edit>,
    pub selection: Range<usize>,
}

impl EditResult {
    /// One edit, the caret left at `caret` in the edited source.
    pub fn one(range: Range<usize>, text: impl Into<String>, caret: usize) -> Self {
        EditResult {
            edits: vec![Edit {
                range,
                text: text.into(),
            }],
            selection: caret..caret,
        }
    }

    /// `source` with the edits applied.
    pub fn apply(&self, source: &str) -> String {
        let edits: Vec<(Range<usize>, String)> = self
            .edits
            .iter()
            .map(|e| (e.range.clone(), e.text.clone()))
            .collect();
        transactions::build_batch(source, &edits)
    }
}
