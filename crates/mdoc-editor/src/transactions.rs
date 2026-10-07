//! Generic source transactions and text-history identity. No host policy.
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEdit {
    pub range: Range<usize>,
    pub new_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryChange {
    pub before: u64,
    pub after: u64,
    /// Sorted edits in pre-operation coordinates; empty for undo/redo travel.
    pub edits: Vec<SourceEdit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorTransaction {
    pub revision: u64,
    pub changes: Vec<HistoryChange>,
    /// Current state and every state reachable through the editor's history.
    pub retained: Vec<u64>,
}

/// Derive exact destination ranges in the same ordered pass as a batch build.
pub fn inverse_edits(edits: &[SourceEdit]) -> Vec<SourceEdit> {
    let mut delta = 0isize;
    edits
        .iter()
        .map(|edit| {
            let start = edit.range.start.saturating_add_signed(delta);
            delta += edit.new_len as isize - edit.range.len() as isize;
            SourceEdit {
                range: start..start + edit.new_len,
                new_len: edit.range.len(),
            }
        })
        .collect()
}

pub(crate) fn build_batch(source: &str, edits: &[(Range<usize>, String)]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditorState, Redo, Undo};
    use gpui::EntityInputHandler;
    #[gpui::test]
    fn exact_history_identity_includes_coalesced_typing_and_ime_commit(
        cx: &mut gpui::TestAppContext,
    ) {
        let (editor, cx) = cx.add_window_view(EditorState::new);
        editor.update_in(cx, |e, window, cx| {
            e.set_text("Anna", cx);
            let initial = e.history_id();
            e.set_cursor(0, cx);
            e.replace_text_in_range(None, "x", window, cx);
            let first = e.history_id();
            let first_tx = e.last_transaction().unwrap().clone();
            assert_eq!(
                first_tx.changes[0].edits[0],
                SourceEdit {
                    range: 0..0,
                    new_len: 1
                }
            );
            e.replace_text_in_range(None, "y", window, cx);
            let second = e.history_id();
            assert_ne!(first, second);
            assert!(!e.last_transaction().unwrap().retained.contains(&first));
            e.undo(&Undo, window, cx);
            assert_eq!(e.history_id(), initial);
            assert_eq!(e.text(), "Anna");
            e.redo(&Redo, window, cx);
            assert_eq!(e.history_id(), second);
            assert_eq!(e.text(), "xyAnna");
            e.set_text("Anna", cx);

            e.set_cursor(0, cx);
            e.replace_and_mark_text_in_range(None, "А", None, window, cx);
            e.replace_and_mark_text_in_range(None, "Ан", None, window, cx);
            let composed = e.history_id();
            e.replace_text_in_range(None, "Ан", window, cx);
            e.undo(&Undo, window, cx);
            assert_eq!(e.history_id(), composed);
            assert_eq!(e.text(), "АнAnna");
        });
    }
    #[test]
    fn batch_output_and_inverse_geometry_are_derived_in_order() {
        let edits = vec![(0..8, "PERSON".into()), (13..21, "PERSON_1".into())];
        assert_eq!(build_batch("Анна and Анна", &edits), "PERSON and PERSON_1");
        assert_eq!(
            inverse_edits(&[
                SourceEdit {
                    range: 0..8,
                    new_len: 6
                },
                SourceEdit {
                    range: 13..21,
                    new_len: 8
                }
            ]),
            vec![
                SourceEdit {
                    range: 0..6,
                    new_len: 8
                },
                SourceEdit {
                    range: 11..19,
                    new_len: 8
                }
            ]
        );
    }
}
