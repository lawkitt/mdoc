//! Markdown table columns that name a part of a person's name (#397): a
//! cell under "| Фамилия |" or "| Surname |" is a name, though alone it has
//! no context the model can read ("| Кий | Ева | Оскаровна |"). Unlike an
//! identifier column's header ([`crate::view`]), a name part's header is
//! not read with its cell: "Фамилия: Кий" makes the model take the header
//! for part of the name. The detector proposes such cells as people
//! instead, each cell a span of its own, as a reader marks them.
use crate::view::{cells, header_name, tables};
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
                && word.split(['-', '\u{2010}']).all(name_part)
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
}
