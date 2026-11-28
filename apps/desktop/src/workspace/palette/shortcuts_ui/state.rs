//! The shortcuts sheet's state: the rows it shows for a filter. No GPUI.

use crate::commands::shortcuts::{self, Group};

/// One shown row: its label and its keys as this platform writes them.
pub(crate) struct Row {
    pub label: &'static str,
    pub keys: Vec<String>,
}

/// The rows the sheet shows for `query`, by group in the sheet's order.
/// Every word of the query must appear in the row's label, its group's
/// title or one of its keys, ignoring case.
pub(crate) fn visible(query: &str) -> Vec<(Group, Vec<Row>)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    Group::ALL
        .into_iter()
        .filter_map(|group| {
            let rows: Vec<Row> = shortcuts::rows()
                .filter(|row| row.group == group)
                .map(|row| Row {
                    label: row.label,
                    keys: row.key_labels(),
                })
                .filter(|row| {
                    let text = format!("{} {} {}", row.label, group.title(), row.keys.join(" "))
                        .to_lowercase();
                    words.iter().all(|word| text.contains(word.as_str()))
                })
                .collect();
            (!rows.is_empty()).then_some((group, rows))
        })
        .collect()
}
