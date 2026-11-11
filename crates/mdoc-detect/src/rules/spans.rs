//! Span patterns: whole spans a `span` entry's pattern proposes, an
//! address from its first element to its postcode or city, a passport's
//! name fields.
use super::alternation::{Alternation, Search};
use super::context::CONTEXT_SCORE;
use super::{Pack, Span, SpanMatch};
use mdoc_pii::{Category, Detection, Recognizer};
use regex::Regex;

impl Pack {
    /// Whether a model span of `category` at `at` of `text`, inside a
    /// `span` pattern's match of `matched` at `within`, is that match's own
    /// part (its `absorbs`), read within the match alone: "Harrow House"
    /// before ", 12 Pellin Street" is the address's building, not a company;
    /// a number after the match ("…, Chatham House, 12 March") is none of it.
    pub(crate) fn absorbs(
        &self,
        text: &str,
        matched: Category,
        within: std::ops::Range<usize>,
        category: Category,
        at: std::ops::Range<usize>,
    ) -> bool {
        self.spans.iter().any(|span| match span {
            Span::Pattern {
                category: of,
                absorbs,
                ..
            } if *of == matched => absorbs.iter().any(|(absorbed, regex)| {
                *absorbed == category
                    && regex
                        .captures(&text[at.start..within.end])
                        .and_then(|found| found.name("span"))
                        .is_some_and(|span| span.range() == (0..at.len()))
            }),
            _ => false,
        })
    }

    /// Whole spans the `span` patterns find in `source`: an address from
    /// its first element to its postcode or city (ADR 0092).
    #[cfg(test)]
    pub(crate) fn spans(&self, source: &str) -> Vec<Detection> {
        self.spans_beside(source, &[])
            .into_iter()
            .map(|found| found.detection)
            .collect()
    }

    /// An address match `range` of `pattern` beside the `names`, or none
    /// (#362). One a name holds at its front edge ("ПЕТРОВ С. ИВАНОВИЧ, УЛ
    /// ЛЕСНАЯ, Д. 5" with the name found, "ООО КЕДРОВСКИЙ РАЙОН, УЛ …")
    /// starts where the pattern finds it again after the name, so neither
    /// is lost; a name the match absorbs ("Harrow House, 12 …") is its own
    /// part. A lone capital that ends it before a capitalised word is that
    /// word's initial, no house letter ("д. 5 В Петров" ends at "д. 5"). One
    /// that ends at "№ 2" before "-1234/2020" is a case number's head.
    fn address_beside(
        &self,
        source: &str,
        names: &[Detection],
        pattern: &Alternation,
        mut range: std::ops::Range<usize>,
    ) -> Option<std::ops::Range<usize>> {
        let front = names
            .iter()
            .filter(|name| {
                matches!(name.category, Category::Person | Category::Organization)
                    && name.range.start <= range.start
                    && range.start < name.range.end
                    && name.range.end < range.end
                    && !self.absorbs(
                        source,
                        Category::Address,
                        range.clone(),
                        name.category,
                        name.range.clone(),
                    )
            })
            .map(|name| name.range.end)
            .max();
        if let Some(after) = front {
            let found = pattern.captures_at(source, after)?;
            let whole = found.get(0).expect("group 0 is the match");
            let start = found.name("lead").map_or(whole.start(), |lead| lead.end());
            let end = found
                .name("tail")
                .map_or(whole.end(), |tail| tail.start())
                .min(range.end);
            if start >= end {
                return None;
            }
            range = start..end;
        }
        let cyrillic_capital = |c: char| ('А'..='Я').contains(&c) || c == 'Ё';
        let mut back = source[range.clone()].chars().rev();
        let after = &source[range.end..];
        if let (Some(letter), Some(space), Some(digit)) = (back.next(), back.next(), back.next())
            && cyrillic_capital(letter)
            && matches!(space, ' ' | '\t')
            && digit.is_ascii_digit()
            && after.starts_with([' ', '\t'])
            && after
                .trim_start_matches([' ', '\t'])
                .starts_with(char::is_uppercase)
        {
            range.end -= letter.len_utf8() + space.len_utf8();
        }
        let text = &source[range.clone()];
        let digits = text.trim_end_matches(|c: char| c.is_ascii_digit());
        let mut ahead = source[range.end..].chars();
        let case_number = digits.len() < text.len()
            && digits.trim_end_matches([' ', '\t']).ends_with('№')
            && matches!(ahead.next(), Some('-' | '/'))
            && ahead.next().is_some_and(|c| c.is_ascii_digit());
        (!case_number && !range.is_empty()).then_some(range)
    }

    /// Matches beside existing evidence, all in view coordinates. Definite
    /// street/building components may contain names; a guessed town tail
    /// never consumes an independent PERSON/ORG or a contact field.
    pub(crate) fn spans_beside(&self, source: &str, names: &[Detection]) -> Vec<SpanMatch> {
        self.spans
            .iter()
            .filter_map(|span| match span {
                Span::Pattern {
                    category,
                    pattern,
                    continuation_after,
                    continuation_stop,
                    split,
                    line_length,
                    skip,
                    gaps_only,
                    ..
                } => Some((
                    *category,
                    pattern,
                    continuation_after,
                    continuation_stop,
                    (split, skip, *gaps_only),
                    line_length,
                )),
                Span::Fix(_) => None,
            })
            .flat_map(
                |(
                    category,
                    pattern,
                    continuation_after,
                    continuation_stop,
                    (split, skip, gaps_only),
                    line_length,
                )| {
                    span_ranges_sized(pattern, source, line_length.as_ref())
                        .into_iter()
                        .map(move |(mut range, whole)| {
                            if let Some(after) = continuation_after.as_ref() {
                                // A match with no definite component (a
                                // city, state and ZIP alone) is guessed
                                // whole: its city takes no name or field
                                // after its start; a name at its front edge
                                // is `address_beside`'s.
                                let core = after.find(&source[range.clone()]);
                                let tail = range.start + core.map_or(0, |core| core.end());
                                // A name crossing the definite component's
                                // edge ("Broadway Show") invalidates the
                                // guessed street; never cut it to a fragment.
                                if core.is_some()
                                    && names.iter().any(|name| {
                                        matches!(
                                            name.category,
                                            Category::Person | Category::Organization
                                        ) && name.range.start < tail
                                            && tail < name.range.end
                                            && range.start < name.range.end
                                    })
                                {
                                    range.end = range.start;
                                    return (range, whole);
                                }
                                let name = names
                                    .iter()
                                    .filter(|name| {
                                        matches!(
                                            name.category,
                                            Category::Person | Category::Organization
                                        ) && (tail < name.range.start
                                            || (core.is_some() && tail == name.range.start))
                                            && name.range.start < range.end
                                    })
                                    .map(|name| name.range.start)
                                    .min();
                                let field = continuation_stop
                                    .as_ref()
                                    .and_then(|pattern| pattern.find(&source[tail..range.end]))
                                    .map(|field| tail + field.start());
                                if let Some(end) = name.into_iter().chain(field).min() {
                                    range.end = range.start
                                        + source[range.start..end]
                                            .trim_end_matches(|c: char| {
                                                c.is_whitespace() || [',', ';', ':'].contains(&c)
                                            })
                                            .len();
                                    // Cutting before a recipient must not
                                    // promote a bare street ("Broadway").
                                    if !self.shaped_named_after(
                                        &source[range.clone()],
                                        category,
                                        &|_| false,
                                        &source[..range.start],
                                    ) {
                                        range.end = range.start;
                                    }
                                }
                            }
                            (range, whole)
                        })
                        // A split match's spans are the pieces between the
                        // split's matches ("KRENC<<LIDIIA" is two names); the
                        // whole match goes with each.
                        .flat_map(move |(range, whole)| match split {
                            Some(split) => pieces(split, source, range)
                                .into_iter()
                                .map(|piece| (piece, Some(whole.clone())))
                                .collect(),
                            None => vec![(range, None)],
                        })
                        .filter_map(move |(range, whole)| {
                            if category != Category::Address || range.is_empty() {
                                return Some((range, whole));
                            }
                            self.address_beside(source, names, pattern, range)
                                .map(|range| (range, whole))
                        })
                        // An address match before a colon ends in a field's
                        // label ("12 Oak Road, Email: …"), not the address's
                        // last part; a name before one is a speaker's ("ФИО1
                        // А. Б.: …").
                        .filter(move |(range, _)| {
                            !range.is_empty()
                                && skip
                                    .as_ref()
                                    .is_none_or(|skip| !skip.is_match(&source[range.clone()]))
                                // A gap filler gives way to any name span of
                                // its category over its text ("Смитом" of
                                // the model in "Великобритании Смитом").
                                && !(gaps_only
                                    && names.iter().any(|name| {
                                        name.category == category
                                            && name.range.start < range.end
                                            && range.start < name.range.end
                                    }))
                                && (category != Category::Address
                                    || !source[range.end..]
                                        .trim_start_matches([' ', '\t'])
                                        .starts_with(':'))
                        })
                        .map(move |(range, whole)| SpanMatch {
                            detection: Detection {
                                range,
                                category,
                                score: CONTEXT_SCORE,
                                recognizer: Recognizer::Span,
                            },
                            whole,
                        })
                },
            )
            .collect()
    }
}

/// A span pattern's matches without their `lead` and `tail` groups, the
/// context around a span ("адресу " before a city). Each search starts
/// where the last span ended, so one address's tail ("; ") can be the next
/// one's lead.
pub(super) fn span_ranges(pattern: &impl Search, source: &str) -> Vec<std::ops::Range<usize>> {
    span_ranges_sized(pattern, source, None)
        .into_iter()
        .map(|(range, _)| range)
        .collect()
}

/// [`span_ranges`] of the matches each of whose lines that is not blank,
/// `lead` and `tail` included, has `line_length` characters, without the
/// whitespace at its ends, its backslashes (Markdown's escapes and hard
/// breaks) and its `mark` groups (a blockquote's or a table's marks, a
/// label), each with its whole match.
fn span_ranges_sized(
    pattern: &impl Search,
    source: &str,
    line_length: Option<&std::ops::RangeInclusive<usize>>,
) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let marks: Vec<&str> = pattern
        .capture_names()
        .flatten()
        .filter(|group| numbered(group, "mark"))
        .collect();
    let mut ranges = Vec::new();
    let mut searcher = pattern.searcher(source);
    let mut at = 0;
    while at <= source.len()
        && let Some(found) = searcher.captures_at(at)
    {
        let whole = found.get(0).expect("group 0 is the match");
        let sized = line_length.is_none_or(|length| {
            let marked: Vec<_> = marks
                .iter()
                .filter_map(|mark| found.name(mark))
                .map(|mark| mark.range())
                .collect();
            let counted: String = whole
                .as_str()
                .char_indices()
                .filter(|(i, c)| {
                    let i = whole.start() + i;
                    *c != '\\' && !marked.iter().any(|mark| mark.contains(&i))
                })
                .map(|(_, c)| c)
                .collect();
            counted
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .all(|line| length.contains(&line.chars().count()))
        });
        let start = found.name("lead").map_or(whole.start(), |lead| lead.end());
        let end = found
            .name("tail")
            .map_or(whole.end(), |tail| tail.start())
            .max(start);
        if !sized {
            at = whole.start()
                + source[whole.start()..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
            continue;
        }
        if start < end {
            ranges.push((start..end, whole.range()));
        }
        at = if end > at {
            end
        } else {
            whole.start()
                + source[whole.start()..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8)
        };
    }
    ranges
}

/// Whether `group` is `base` or `base` with a number (`mark`, `mark2`).
pub(super) fn numbered(group: &str, base: &str) -> bool {
    group
        .strip_prefix(base)
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
}

/// The pieces of `range` in `source` between the matches of `split`, none
/// empty: "KRENC", "LIDIIA" and "ARSENEVNA" of "KRENC<<LIDIIA<ARSENEVNA".
fn pieces(
    split: &Regex,
    source: &str,
    range: std::ops::Range<usize>,
) -> Vec<std::ops::Range<usize>> {
    let mut pieces = Vec::new();
    let mut start = range.start;
    for cut in split.find_iter(&source[range.clone()]) {
        pieces.push(start..range.start + cut.start());
        start = range.start + cut.end();
    }
    pieces.push(start..range.end);
    pieces.retain(|piece| !piece.is_empty());
    pieces
}
