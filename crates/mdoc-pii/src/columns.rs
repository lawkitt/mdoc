//! Markdown table columns that name a part of a person's name (#397): a
//! cell under "| Фамилия |" or "| Surname |" is a name, though alone it has
//! no context the model can read ("| Кий | Ева | Оскаровна |"). Unlike an
//! identifier column's header ([`crate::view`]), a name part's header is
//! not read with its cell: "Фамилия: Кий" makes the model take the header
//! for part of the name. The detector proposes such cells as people
//! instead, each cell a span of its own, as a reader marks them. A value
//! after a name part's label in text ("Surname: McVellin", side by side in
//! an OCR'd two-column form) is proposed the same way ([`labelled_names`]).
use crate::view::{cells, fence, header_name, tables};
use std::ops::Range;

/// Headers that name a part of a person's name, as [`header_name`] writes
/// them (lowercase, "ё" as "е", without `*`, `:`, `#` and `№`), matched
/// whole. "Ф.И.О." matches however it is spaced. A bare "Name" counts only
/// beside one of these: alone it heads companies and files as often.
const NAME_HEADERS: &[&str] = &[
    // Russian.
    "фамилия",
    "имя",
    "отчество",
    "фио",
    "фамилия, имя, отчество",
    "фамилия имя отчество",
    "фамилия и имя",
    "имя и отчество",
    "имя, отчество",
    // English.
    "surname",
    "surnames",
    "last name",
    "family name",
    "first name",
    "first names",
    "given name",
    "given names",
    "given name(s)",
    "forename",
    "forenames",
    "middle name",
    "middle names",
    "patronymic",
    "full name",
];

/// Cells that are no name, lowercase: totals rows and placeholders.
const NOT_NAMES: &[&str] = &[
    "итого",
    "всего",
    "нет",
    "не указано",
    "не указан",
    "не указана",
    "отсутствует",
    "неизвестно",
    "аноним",
    "total",
    "totals",
    "subtotal",
    "unknown",
    "none",
    "anonymous",
    "redacted",
    "withheld",
    "vacant",
    "tbd",
    "tba",
    "same",
    "ditto",
];

/// Lowercase words a name may hold after its first ("Ana de la Vela").
const PARTICLES: &[&str] = &[
    "de", "da", "di", "du", "del", "della", "der", "den", "van", "von", "la", "le", "bin", "ibn",
    "al", "ben", "ter", "ten", "y", "оглы", "кызы", "ибн", "фон", "ван", "де",
];

/// Whether a header cell names a name part.
fn name_header(header: &str) -> bool {
    let name = header_name(header);
    let compact: String = name.chars().filter(|c| !matches!(c, '.' | ' ')).collect();
    NAME_HEADERS.contains(&name.as_str()) || compact == "фио"
}

/// The source ranges of the cells under a name part's header that look like
/// a name: one to four words of letters, each capitalised (a particle
/// aside), with hyphens and apostrophes inside ("O’Kell", "Мамин-Сибиряк"),
/// no digit ("ФИО1" is a placeholder) and no other mark. Emphasis around
/// the value stays outside its range. A totals row, a placeholder or a
/// header repeated in the body gives none. The detector's own filters still
/// judge each (a role word is no name).
pub fn name_cells(source: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    for table in tables(source) {
        let (_, header) = table.header;
        let titles: Vec<String> = table
            .columns
            .iter()
            .map(|cell| header_name(&header[cell.clone()]))
            .collect();
        let parts: Vec<bool> = table
            .columns
            .iter()
            .map(|cell| name_header(&header[cell.clone()]))
            .collect();
        if !parts.contains(&true) {
            continue;
        }
        let columns: Vec<bool> = parts
            .iter()
            .zip(&titles)
            .map(|(part, title)| *part || title == "name")
            .collect();
        for &(start, line) in &table.rows {
            for (cell, column) in cells(line).into_iter().zip(&columns) {
                if !column {
                    continue;
                }
                let value = unwrapped(&line[cell.clone()]);
                let text = &line[value.start + cell.start..value.end + cell.start];
                let lower = header_name(text);
                if name_like(text)
                    && !NOT_NAMES.contains(&lower.as_str())
                    && !titles.contains(&lower)
                    && !name_header(text)
                {
                    let at = start + cell.start;
                    out.push(at + value.start..at + value.end);
                }
            }
        }
    }
    out
}

/// The range of `cell` within it without its spaces and the emphasis
/// around its value ("**Кий**").
fn unwrapped(cell: &str) -> Range<usize> {
    let mut range = 0..cell.len();
    loop {
        let text = &cell[range.clone()];
        let trimmed = text.trim();
        range.start += text.len() - text.trim_start().len();
        range.end = range.start + trimmed.len();
        let Some(mark) = ["**", "__", "*", "_"].into_iter().find(|mark| {
            trimmed.len() > 2 * mark.len() && trimmed.starts_with(mark) && trimmed.ends_with(mark)
        }) else {
            return range;
        };
        range.start += mark.len();
        range.end -= mark.len();
    }
}

/// Whether `value` reads as a name: see [`name_cells`].
fn name_like(value: &str) -> bool {
    let words: Vec<&str> = value.split(' ').collect();
    (1..=4).contains(&words.len())
        && words.iter().enumerate().all(|(i, word)| {
            if i > 0 && PARTICLES.contains(word) {
                return true;
            }
            word.starts_with(char::is_uppercase)
                && word.chars().count() <= 40
                && word
                    .split(['-', '\u{2010}', '\u{2011}', '\u{2013}'])
                    .all(name_part)
        })
}

/// One hyphen-free part of a name's word: letters with apostrophes inside,
/// at least two letters, not one letter repeated ("XXX").
fn name_part(part: &str) -> bool {
    let apostrophe = |c: char| matches!(c, '\'' | '’' | 'ʼ');
    let letters: Vec<char> = part
        .chars()
        .filter(|c| !apostrophe(*c))
        .flat_map(char::to_lowercase)
        .collect();
    part.starts_with(char::is_alphabetic)
        && part.ends_with(char::is_alphabetic)
        && part.chars().all(|c| c.is_alphabetic() || apostrophe(c))
        && letters.len() >= 2
        && letters.iter().any(|c| *c != letters[0])
}

/// Labels in text that name a part of a person's name, lowercase, the
/// longer of two that share a start first. "Name" alone is none: it labels
/// companies and files as often.
const NAME_LABELS: &[&str] = &[
    "given name(s)",
    "given names",
    "given name",
    "first names",
    "first name",
    "forename(s)",
    "forenames",
    "forename",
    "middle name(s)",
    "middle names",
    "middle name",
    "surname",
    "last name",
    "family name",
    "full name",
    "patronymic",
    "фамилия",
    "имя",
    "отчество",
];

/// The source ranges of the values after a name part's label and a colon
/// that look like a name, as [`name_cells`] judges a cell: "Given name:
/// Alvora      Given name: Nerian" gives "Alvora" and "Nerian". A value
/// ends at the line's end, at two spaces or a tab (the next column), at a
/// "|" or before the next label. A label counts at the start of a line or
/// a column, or right after the value before it: in prose ("the surname:
/// is required") it is none. Code blocks, code spans and link targets are
/// left alone.
pub fn labelled_names(source: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut in_fence: Option<&str> = None;
    for raw in source.split_inclusive('\n') {
        let start = at;
        at += raw.len();
        let line = raw.trim_end_matches(['\r', '\n']);
        if let Some(open) = in_fence {
            if line.trim_start().starts_with(open) {
                in_fence = None;
            }
            continue;
        }
        if let Some(open) = fence(line) {
            in_fence = Some(open);
            continue;
        }
        let labels = labels(line);
        for (i, label) in labels.iter().enumerate() {
            let anchored = match i.checked_sub(1).map(|j| &labels[j]) {
                Some(before) if before.value_end == label.start => true,
                _ => column_start(&line[..label.start]),
            };
            if !anchored || in_code_or_target(&line[..label.start]) {
                continue;
            }
            let region = &line[label.value_start..label.value_end];
            let value = unwrapped(region);
            let text = &region[value.clone()];
            let lower = header_name(text);
            if name_like(text) && !NOT_NAMES.contains(&lower.as_str()) && !name_header(text) {
                let from = start + label.value_start;
                out.push(from + value.start..from + value.end);
            }
        }
    }
    out
}

/// A name part's label on a line: where it starts, and the region its
/// value may take after its colon.
struct Label {
    start: usize,
    value_start: usize,
    value_end: usize,
}

/// The name part's labels on `line`, each a whole word a colon follows
/// (emphasis and spaces between), with its value's region up to the next
/// label, a column gap, a "|" or the line's end.
fn labels(line: &str) -> Vec<Label> {
    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut boundary = true;
    for (i, c) in line.char_indices() {
        if boundary
            && let Some(colon) = NAME_LABELS
                .iter()
                .find_map(|label| label_at(&line[i..], label))
        {
            found.push((i, i + colon));
        }
        boundary = !c.is_alphanumeric();
    }
    let mut out: Vec<Label> = Vec::new();
    for (k, &(start, value_start)) in found.iter().enumerate() {
        if out.last().is_some_and(|last| start < last.value_start) {
            continue;
        }
        let limit = found
            .get(k + 1)
            .map_or(line.len(), |&(next, _)| next)
            .max(value_start);
        // The spaces or tab after the colon are no column gap.
        let region = &line[value_start..limit];
        let lead = region.len() - region.trim_start_matches([' ', '\t']).len();
        let region = &region[lead..];
        let end = [region.find("  "), region.find('\t'), region.find('|')]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(region.len());
        out.push(Label {
            start,
            value_start,
            value_end: value_start + lead + end,
        });
    }
    out
}

/// The length of `label` at the start of `text`, matched ignoring case as a
/// whole word, through the colon after it and the emphasis that closes it
/// ("**Surname:**"), or `None`.
fn label_at(text: &str, label: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    for want in label.chars() {
        let (_, c) = chars.next()?;
        if !c.to_lowercase().eq(std::iter::once(want)) {
            return None;
        }
    }
    let rest = &text[chars.next().map_or(text.len(), |(i, _)| i)..];
    if rest.starts_with(char::is_alphanumeric) {
        return None;
    }
    let after = rest.trim_start_matches(['*', '_', ' ']);
    let after = after.strip_prefix(':')?;
    let after = after.trim_start_matches(['*', '_']);
    Some(text.len() - after.len())
}

/// Whether a label after `before` starts a line or a column: only spaces,
/// a list marker or emphasis on the line before it, or two spaces or a tab
/// right before it.
fn column_start(before: &str) -> bool {
    let lead = before.trim_end_matches(['*', '_']);
    lead.trim_start_matches([' ', '\t', '-', '*', '+', '>', '|'])
        .is_empty()
        || lead.ends_with("  ")
        || lead.ends_with('\t')
        || lead.trim_end_matches(' ').ends_with('|')
}

/// Whether a label after `before` on its line sits in a code span (an odd
/// number of backticks before it) or a link's target (a "](" not yet
/// closed).
fn in_code_or_target(before: &str) -> bool {
    before.matches('`').count() % 2 == 1
        || before
            .rfind("](")
            .is_some_and(|open| !before[open..].contains(')'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(source: &str) -> Vec<&str> {
        name_cells(source)
            .into_iter()
            .map(|range| &source[range])
            .collect()
    }

    /// hunt-ru-r2-09: each name part's cell is a name; the passport column
    /// is not a name column.
    #[test]
    fn name_part_columns_give_their_cells() {
        let source = "| Фамилия | Имя | Отчество | Паспорт |\n\
                      | --- | --- | --- | --- |\n\
                      | Кий | Ева | Оскаровна | 46 82 731609 |\n\
                      | Лю | **Леон** | Янович | 46 83 862710 |\n";
        assert_eq!(
            found(source),
            ["Кий", "Ева", "Оскаровна", "Лю", "Леон", "Янович"]
        );
        let source = "| Given name | Surname | Initials | Function |\n\
                      |---|---|---|---|\n\
                      | Avelina | McVellin | A. M. | Witness |\n\
                      | Nerovan | O’Kell | N. O. | Recipient |\n";
        assert_eq!(found(source), ["Avelina", "McVellin", "Nerovan", "O’Kell"]);
    }

    #[test]
    fn headers_match_whole_however_written() {
        for header in [
            "Фамилия",
            "**ФИО**",
            "Ф.И.О.",
            "Ф. И. О.:",
            "Фамилия, имя, отчество",
            "Surname",
            "Last Name",
            "Family name",
            "First name",
            "Given name(s)",
            "Middle name",
            "Patronymic",
            "Full name",
        ] {
            assert!(name_header(header), "{header}");
        }
        for header in [
            "Name",
            "Фамилия кредитора",
            "Company name",
            "File name",
            "Паспорт",
            "Initials",
            "Role",
        ] {
            assert!(!name_header(header), "{header}");
        }
    }

    #[test]
    fn cell_shapes() {
        for name in [
            "Кий",
            "Лю",
            "O’Kell",
            "O'Neill",
            "Мамин-Сибиряк",
            "McVellin",
            "Ана де Вела",
            "Кий Ева Оскаровна",
            "КИЙ",
        ] {
            assert!(name_like(name), "{name}");
        }
        for value in [
            "ФИО1",
            "Ivan2",
            "A. M.",
            "А.",
            "Х",
            "XXX",
            "кий",
            "де Вела",
            "Ivan  Petrov",
            "Ivan, Petrov",
            "Ivan Petrov Sergeevich Ivanov Jr",
            "O’",
            "-Ivan",
            "&#1050;ий",
            "—",
            "",
        ] {
            assert!(!name_like(value), "{value}");
        }
    }

    /// No digits, placeholders, totals, repeated headers; no table, no body,
    /// a "Name" column alone and a column outside the name parts give
    /// nothing.
    #[test]
    fn negatives() {
        let source = "| ФИО | Роль |\n\
                      | --- | --- |\n\
                      | ФИО1 | Истец |\n\
                      | Не указано | Свидетель |\n\
                      | Итого | 2 |\n\
                      | ФИО | Роль |\n\
                      | Ф.И.О. | — |\n\
                      |  | Эксперт |\n";
        assert!(found(source).is_empty());
        // A "Name" column lists companies and files unless a name part's
        // header stands beside it.
        assert!(found("| Name | Type |\n| --- | --- |\n| Orvel Lanmark | Company |\n| Ledger Notes | File |\n").is_empty());
        assert_eq!(
            found("| Name | Surname |\n| --- | --- |\n| Orvel | Lanmark |\n"),
            ["Orvel", "Lanmark"]
        );
        // A header row alone, cells outside a table and a code block's
        // table.
        assert!(found("| Фамилия | Имя |\n| --- | --- |\n").is_empty());
        assert!(found("| Фамилия | Имя |\n| Кий | Ева |\n").is_empty());
        assert!(found("Фамилия: Кий. Имя: Ева.\n\nКий | Ева\n").is_empty());
        assert!(found("```\n| Surname |\n| --- |\n| Lanmark |\n```\n").is_empty());
        // A blank line ends the table.
        assert!(found("| Surname |\n| --- |\n\n| Lanmark |\n").is_empty());
    }

    #[test]
    fn offsets_cover_the_value_only() {
        let source = "Intro\n\n| Surname |\n| --- |\n|  __Lanmark__  |\n";
        let ranges = name_cells(source);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&source[ranges[0].clone()], "Lanmark");
        assert_eq!(ranges[0].start, source.find("Lanmark").unwrap());
    }

    fn labelled(source: &str) -> Vec<&str> {
        labelled_names(source)
            .into_iter()
            .map(|range| &source[range])
            .collect()
    }

    /// hunt-en-r6-06: an OCR'd two-column form, label and value side by
    /// side on one line.
    #[test]
    fn labelled_values_one_and_two_columns() {
        assert_eq!(labelled("Surname: McVellin\n"), ["McVellin"]);
        let source = "LEFT COLUMN: PRINCIPAL                  RIGHT COLUMN: ATTORNEY\n\
                      Given name: Alvora                        Given name: Nerian\n\
                      Surname: McVellin                        Surname: O’Kell–Venn\n\
                      Phone: +44 7700 900482                  Phone: +44 7700 900593\n";
        assert_eq!(
            labelled(source),
            ["Alvora", "Nerian", "McVellin", "O’Kell–Venn"]
        );
        let source = "First name:\tAna de la Vela\tLast name: O'Neill\r\n";
        assert_eq!(labelled(source), ["Ana de la Vela", "O'Neill"]);
        // Another label ends a value; emphasis stays outside.
        let source = "- **Forename:** Orvel Surname: __Lanmark__ | Middle name(s): Ivo\n";
        assert_eq!(labelled(source), ["Orvel", "Lanmark", "Ivo"]);
        let ranges = labelled_names(source);
        assert_eq!(ranges[1].start, source.find("Lanmark").unwrap());
    }

    #[test]
    fn labelled_values_russian() {
        let source = "Фамилия: Кий    Имя: Ева    Отчество: Оскаровна\n\
                      ФАМИЛИЯ: Мамин-Сибиряк\n";
        assert_eq!(
            labelled(source),
            ["Кий", "Ева", "Оскаровна", "Мамин-Сибиряк"]
        );
    }

    #[test]
    fn labelled_negatives() {
        for source in [
            "Name: Acme Ltd\n",
            "File name: Ledger Notes\n",
            "In this form the surname: is required.\n",
            "The applicant's Surname: Lanmark\n",
            "Surname: Ivan2\n",
            "Фамилия: ФИО1\n",
            "Surname: Unknown\n",
            "Surname: XXX\n",
            "Surname: lanmark\n",
            "Surnames listed: Lanmark\n",
            "Full name: Orvel Ivo Lanmark Petrov Jr\n",
            "`Surname: Lanmark`\n",
            "[form](https://x.test/?Surname: Lanmark)\n",
            "```\nSurname: Lanmark\n```\n",
            "Surname:\n",
        ] {
            assert!(labelled(source).is_empty(), "{source}");
        }
        // An empty value before the next column's label gives only that
        // column's value.
        assert_eq!(
            labelled("Given name:        Given name: Nerian\n"),
            ["Nerian"]
        );
        assert_eq!(labelled("Given name: Surname: Lanmark\n"), ["Lanmark"]);
        // The fence closes: a label after it counts.
        assert_eq!(
            labelled("~~~\nSurname: A\n~~~\nSurname: Lanmark\n"),
            ["Lanmark"]
        );
    }
}
