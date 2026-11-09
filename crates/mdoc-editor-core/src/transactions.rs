//! Batch text construction for source edits.

use std::ops::Range;

/// `source` with sorted, non-overlapping `edits` applied, each range in the
/// offsets of the original source.
pub fn build_batch(source: &str, edits: &[(Range<usize>, String)]) -> String {
    let mut result = String::with_capacity(source.len());
    let mut end = 0;
    for (range, replacement) in edits {
        result.push_str(&source[end..range.start]);
        result.push_str(replacement);
        end = range.end;
    }
    result.push_str(&source[end..]);
    result
}
