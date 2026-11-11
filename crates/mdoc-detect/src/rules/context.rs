//! Labelled values: `context` rules, which propose a value beside its
//! label, table columns headed by a label, and identifier fields' labels.
use super::{Context, FieldLabel, Pack, Span};
use mdoc_pii::{Category, Detection, Recognizer};

/// A labelled value's score: rule evidence, like the structured rules'.
pub(super) const CONTEXT_SCORE: f32 = 0.95;

/// `text` without the spaces and inline markup at its end (emphasis,
/// strike-through, code, and a backslash escaping one of them), then at
/// most one line break and the same again: the text whose last word is the
/// one before a label ("**исполнительное** производство", a wrapped
/// "исполнительного" above "производства"). Across a blank line it ends
/// with a line break, so the label has no word before it ("Сводное", a
/// blank line, "**Производство"). A backslash before a space is no markup.
fn word_before(text: &str) -> &str {
    fn inline(mut text: &str) -> &str {
        loop {
            let trimmed = text.trim_end_matches([' ', '\t', '\u{a0}']);
            match trimmed.strip_suffix(['*', '_', '~', '`']) {
                Some(rest) => text = rest.strip_suffix('\\').unwrap_or(rest),
                None => return trimmed,
            }
        }
    }
    let text = inline(text);
    match text.strip_suffix('\n') {
        Some(line) => inline(line.strip_suffix('\r').unwrap_or(line)),
        None => text,
    }
}

impl Pack {
    /// Where the label before the `context` value at `value` starts, on the
    /// value's line of `source` ("паспорт" of "Иванов Иван, паспорт 4510
    /// 123456"); `None` for a value with no label before it on its line (a
    /// table column's cell, a label after its value).
    pub(crate) fn label_start(
        &self,
        source: &str,
        value: &std::ops::Range<usize>,
    ) -> Option<usize> {
        let line = source[..value.start].rfind('\n').map_or(0, |at| at + 1);
        let text = &source[line..value.end];
        self.contexts
            .iter()
            .filter(|context| !context.after)
            .find_map(|context| {
                context.pattern.captures_iter(text).find_map(|found| {
                    let at = found.get(1)?;
                    (line + at.start() == value.start && line + at.end() == value.end)
                        .then(|| line + found.get(0).map_or(at.start(), |whole| whole.start()))
                })
            })
    }

    /// Values beside their labels in `source`, as `context` rules propose
    /// them: the value only, without its label.
    pub(crate) fn context(&self, source: &str) -> Vec<Detection> {
        let mut found: Vec<Detection> = self
            .contexts
            .iter()
            .flat_map(|context| {
                context.pattern.captures_iter(source).filter_map(|found| {
                    let value = found.get(1)?;
                    // The value ends where a word does ("1234_5678" is one word).
                    let ends = !source[value.end()..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');
                    // A value before its label starts after a space, a
                    // quote or a bracket: "23 March", not "123 March", nor
                    // a number's tail ("AB-12/04/1979", "No.12/04/1979").
                    let starts = !context.after
                        || source[..value.start()]
                            .chars()
                            .next_back()
                            .is_none_or(|c| c.is_whitespace() || "([«\"'“‘„".contains(c));
                    // A word `only_after` lists just before it, on its line
                    // or the one above ("a March 9, 1984 birth date"; a
                    // possessive for "'s": "the claimant's").
                    let led = context.only_after.is_empty() || {
                        let text = &source[..value.start()];
                        let text = text.trim_end_matches([' ', '\t', '\u{a0}']);
                        let text = text.strip_suffix('\n').unwrap_or(text);
                        let text = text.strip_suffix('\r').unwrap_or(text);
                        let text = text.trim_end_matches([' ', '\t', '\u{a0}']);
                        let word = &text[text
                            .trim_end_matches(|c: char| c.is_alphanumeric() || "'’".contains(c))
                            .len()..];
                        let word = word.to_lowercase();
                        context.only_after.contains(&word)
                            || (context.only_after.iter().any(|w| w == "'s")
                                && word.chars().count() > 2
                                && (word.ends_with("'s") || word.ends_with("’s")))
                    };
                    // What follows an `after` label: "г.р." closes with a
                    // dot, "birth date" ends its phrase.
                    let followed = context.follow.as_ref().is_none_or(|follow| {
                        found
                            .get(0)
                            .is_some_and(|whole| follow.is_match(&source[whole.end()..]))
                    });
                    // A date wrapped at a line break before its label ends
                    // its line, or punctuation follows it: "born March
                    // 9,\n1984, in Zenda", never the next line's number
                    // ("4 July\n1600 Pennsylvania Avenue") or words ("14
                    // March\n2026 hearing listed").
                    let wrapped = context.after
                        || !value.as_str().contains('\n')
                        || source[value.end()..]
                            .trim_start_matches([' ', '\t', '\u{a0}'])
                            .chars()
                            .next()
                            .is_none_or(|c| "\r\n.,;:)]»\"'”’".contains(c));
                    let start = found.get(0)?.start();
                    // The word before the label, past spaces and inline
                    // markup closing it ("**исполнительное** производство",
                    // "_in_ case"), on its line or the one above, never
                    // across a blank line.
                    let before = word_before(&source[..start])
                        .rsplit(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap_or_default()
                        .to_lowercase();
                    // Only whitespace and Markdown marks before a line's
                    // label, and no line just above it that ends in a comma:
                    // there a wrapped sentence or citation goes on ("Grant
                    // v. Finecy,\nNo. 02-23-00310-CV, 2031 Tex. App. LEXIS").
                    let opens = || {
                        let mut lines = source[..start].rsplit('\n');
                        lines
                            .next()
                            .unwrap_or_default()
                            .chars()
                            .all(|c| c.is_whitespace() || "#>*_|".contains(c))
                            && !lines
                                .next()
                                .is_some_and(|line| line.trim_end().ends_with(','))
                    };
                    // An initialism's label never continues another one:
                    // "C.A. No." inside "18 U.S.C.A. No. 1234-5".
                    let mut label = source[start..].chars();
                    let mut back = source[..start].chars().rev();
                    let inside = !context.after
                        && label.next().is_some_and(char::is_alphabetic)
                        && label.next() == Some('.')
                        && back.next() == Some('.')
                        && back.next().is_some_and(char::is_alphabetic);
                    // A label joined to another by a slash ("ИНН/ОГРН
                    // 7701234567/1027700132195") leaves the run to the
                    // model: its value may be the other label's.
                    let slashed = context.unless_slashed.as_ref().is_some_and(|label| {
                        let blank = [' ', '\t', '\u{a0}'];
                        let end = label.find(&source[start..]).map_or(0, |found| found.end());
                        source[..start].trim_end_matches(blank).ends_with('/')
                            || source[start + end..]
                                .trim_start_matches(blank)
                                .starts_with('/')
                    });
                    if !ends
                        || !starts
                        || !led
                        || !followed
                        || !wrapped
                        || inside
                        || slashed
                        || context.unless_after.contains(&before)
                        || (context.line_start && !opens())
                        || !self.takes(context, value.as_str())
                    {
                        return None;
                    }
                    Some(Detection {
                        range: value.range(),
                        category: context.category,
                        score: CONTEXT_SCORE,
                        // The review limits a date of birth's repeats by the
                        // label on the same side (#466, #363).
                        recognizer: if context.after {
                            Recognizer::ContextAfter
                        } else {
                            Recognizer::Context
                        },
                    })
                })
            })
            .collect();
        found.extend(self.table_columns(source));
        found
    }

    /// Whether `context` proposes `value`: enough digits, no guess that rules
    /// it out (a date), and the guess it needs if it names one (a date of
    /// birth must look like a date).
    fn takes(&self, context: &Context, value: &str) -> bool {
        let guessed = self.guess(value);
        value.chars().filter(char::is_ascii_digit).count() >= context.min_digits
            && !guessed.is_some_and(|guessed| context.unless_guessed.contains(&guessed))
            && (context.only_guessed.is_empty()
                || guessed.is_some_and(|guessed| context.only_guessed.contains(&guessed)))
    }

    /// Values in Markdown table columns whose header cell is a label ("| ИНН
    /// | СНИЛС |"): each cell that is a whole value of that label's rule.
    fn table_columns(&self, source: &str) -> Vec<Detection> {
        let mut found = Vec::new();
        let mut lines = Vec::new();
        let mut at = 0;
        for line in source.split_inclusive('\n') {
            lines.push((at, line.trim_end_matches(['\n', '\r'])));
            at += line.len();
        }
        let mut row = 0;
        while row + 1 < lines.len() {
            let (header_at, header) = lines[row];
            if !is_table_row(header) || !is_delimiter_row(lines[row + 1].1) {
                row += 1;
                continue;
            }
            // The context rule each column's header names, if any.
            let columns: Vec<Option<&Context>> = cells(header, header_at)
                .into_iter()
                .map(|cell| {
                    let label = source[cell]
                        .trim_matches(|c: char| {
                            c == '*' || c == '_' || c == ':' || c.is_whitespace()
                        })
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase();
                    self.contexts
                        .iter()
                        .find(|context| !context.after && context.labels.contains(&label))
                })
                .collect();
            row += 2;
            // Up to the next table's header, which a delimiter row follows.
            while row < lines.len()
                && is_table_row(lines[row].1)
                && !lines
                    .get(row + 1)
                    .is_some_and(|(_, next)| is_delimiter_row(next))
            {
                let (line_at, line) = lines[row];
                for (cell, context) in cells(line, line_at).into_iter().zip(&columns) {
                    if let Some(context) = context
                        && context.value.is_match(&source[cell.clone()])
                        && self.takes(context, &source[cell.clone()])
                    {
                        found.push(Detection {
                            range: cell,
                            category: context.category,
                            score: CONTEXT_SCORE,
                            recognizer: Recognizer::Context,
                        });
                    }
                }
                row += 1;
            }
        }
        found
    }

    /// What a field's label says of the value after it (#739): its
    /// `label` words, an identifier `context`'s labels or an identifier
    /// span fix's `trim` words, as `label_words` reads them ("Тел.:" is
    /// "тел"), ending the words given, the longest first ("паспорт серии"
    /// by "серии"). A `label` entry's words come first, so "счёт" is
    /// `strict` though BANK trims it. None when no label ends the words or
    /// an `only_after` label follows other words. The number sign is no
    /// label.
    pub(crate) fn field_label(&self, words: &str) -> Option<FieldLabel<'_>> {
        let words = label_words(words);
        let words: Vec<&str> = words.split(' ').filter(|w| !w.is_empty()).collect();
        for from in 0..words.len() {
            let label = words[from..].join(" ");
            if let Some(found) = self.labels.iter().find(|found| found.words == label) {
                let before = &words[..from];
                return if found.only_after.is_empty() {
                    Some(if found.strict {
                        FieldLabel::Strict(found.category)
                    } else {
                        FieldLabel::Labelled(found.category, None)
                    })
                } else if before.is_empty() {
                    Some(FieldLabel::Strict(found.category))
                } else if before
                    .iter()
                    .any(|word| found.only_after.iter().any(|after| after == word))
                {
                    Some(if found.strict {
                        FieldLabel::Strict(found.category)
                    } else {
                        FieldLabel::Labelled(found.category, None)
                    })
                } else {
                    None
                };
            }
            let same = |words: &str| label_words(words) == label;
            if let Some(context) = self
                .contexts
                .iter()
                .filter(|context| identifier_field(context.category))
                .find(|context| context.labels.iter().any(|words| same(words)))
            {
                return Some(FieldLabel::Labelled(context.category, Some(&context.value)));
            }
            if let Some(category) = self.spans.iter().find_map(|span| match span {
                Span::Fix(fix)
                    if identifier_field(fix.category)
                        && fix.trim.iter().any(|words| same(words)) =>
                {
                    Some(fix.category)
                }
                _ => None,
            }) {
                return Some(FieldLabel::Labelled(category, None));
            }
        }
        None
    }
}

fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// Whether an organization or person span is a field code heading a
/// Markdown table's column ("| FLDA | FLDB |" over a delimiter row): a
/// whole header cell of capitals, digits and underscores, up to ten, in a
/// header of two cells or more that are all such codes, a list of fields,
/// not of companies (guide rule 8, issue #501). A header with any other
/// cell keeps its spans ("| HSBC | Quillon Freight LLC |", "| IBM | Price
/// |").
pub(super) fn field_code(source: &str, detection: &Detection) -> bool {
    let code = |text: &str| {
        (2..=10).contains(&text.len())
            && text.starts_with(|c: char| c.is_ascii_uppercase())
            && text
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    };
    let range = detection.range.clone();
    let text = source[range.clone()].trim();
    if !matches!(
        detection.category,
        Category::Organization | Category::Person
    ) || !code(text)
    {
        return false;
    }
    let start = source[..range.start].rfind('\n').map_or(0, |at| at + 1);
    let end = source[range.end..]
        .find('\n')
        .map_or(source.len(), |at| range.end + at);
    let line = &source[start..end];
    let next = source[end..]
        .strip_prefix('\n')
        .map(|rest| rest.split('\n').next().unwrap_or_default());
    let header: Vec<(std::ops::Range<usize>, &str)> = cells(line, start)
        .into_iter()
        .map(|cell| {
            let text = source[cell.clone()].trim_matches(['*', '`', ' ', '\r']);
            (cell, text)
        })
        .filter(|(_, text)| !text.is_empty())
        .collect();
    is_table_row(line)
        && next.is_some_and(is_delimiter_row)
        && header.len() >= 2
        && header.iter().all(|(_, cell)| code(cell))
        && header.iter().any(|(cell, cell_text)| {
            cell.start <= range.start && range.end <= cell.end && *cell_text == text
        })
}

/// "| --- | :-: |": the row under a table's header.
fn is_delimiter_row(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('|')
        && line.contains('-')
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// The byte ranges of a table row's cells in the source, each trimmed of
/// spaces; an escaped `\|` stays inside its cell.
fn cells(line: &str, at: usize) -> Vec<std::ops::Range<usize>> {
    let mut cells = Vec::new();
    let mut start = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if c == '|' && !escaped {
            if let Some(from) = start {
                let text: &str = &line[from..i];
                let lead = text.len() - text.trim_start().len();
                let trimmed = text.trim();
                cells.push(at + from + lead..at + from + lead + trimmed.len());
            }
            start = Some(i + 1);
        }
        escaped = c == '\\' && !escaped;
    }
    // A trailing pipe is optional: the rest of the line is the last cell.
    if let Some(from) = start {
        let text = &line[from..];
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let lead = text.len() - text.trim_start().len();
            cells.push(at + from + lead..at + from + lead + trimmed.len());
        }
    }
    cells
}

/// Whether `category` is an identifier field's, which a `label` names.
pub(crate) fn identifier_field(category: Category) -> bool {
    matches!(
        category,
        Category::Tax | Category::Bank | Category::Identity | Category::Phone
    )
}

/// A label as `label` entries write it: lowercase words, single-spaced,
/// without their dots and edge punctuation ("Тел.:" is "тел", "р/с" stays).
pub(crate) fn label_words(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .replace('.', "")
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
