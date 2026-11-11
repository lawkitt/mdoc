//! Span patterns: whole spans a `span` entry's pattern proposes, an
//! address from its first element to its postcode or city, a passport's
//! name fields.
use super::alternation::{Alternation, Search};
use super::context::CONTEXT_SCORE;
use super::{Pack, Span, SpanMatch};
use mdoc_pii::{Category, Detection, Recognizer};
use regex::Regex;

impl Pack {
    /// Whether a model span of `category` at `at` of `text`, inside the
    /// match at `within` of the `span` entry numbered `entry`, is that
    /// match's own part (the entry's own `absorbs`, no other entry's), read
    /// within the match alone: "Harrow House" before ", 12 Pellin Street" is
    /// the address's building, not a company; a number after the match
    /// ("…, Chatham House, 12 March") is none of it.
    pub(crate) fn absorbs(
        &self,
        text: &str,
        entry: usize,
        within: std::ops::Range<usize>,
        category: Category,
        at: std::ops::Range<usize>,
    ) -> bool {
        self.spans.get(entry).is_some_and(|span| match span {
            Span::Pattern { absorbs, .. } => absorbs.iter().any(|(absorbed, regex)| {
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
        self.spans_beside(source, &[], &[])
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
    /// A `city` the rule found after a body's name ([`Self::guessed_city`])
    /// gives way only to a name that starts with it.
    fn address_beside(
        &self,
        source: &str,
        names: &[Detection],
        (entry, pattern): (usize, &Alternation),
        mut range: std::ops::Range<usize>,
        city: bool,
    ) -> Option<std::ops::Range<usize>> {
        let front = names
            .iter()
            .filter(|name| {
                matches!(name.category, Category::Person | Category::Organization)
                    && name.range.start <= range.start
                    && !(city && name.range.start < range.start)
                    && range.start < name.range.end
                    && name.range.end < range.end
                    && !self.absorbs(
                        source,
                        entry,
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

    /// A city, state and ZIP with no street or box before them, the match
    /// at `range` of `pattern` (its lead included in `whole`), guessed
    /// whole: its city takes no name or `stop` field after its start, and
    /// the pattern is searched again after them ("Ship To Ashby Falls, OH
    /// 45999"); a name at its front edge is `address_beside`'s. A model
    /// name over the city's first words, kept or dropped as a public body
    /// ("Bureau of Records" before "Haddon, VA 20999"), is none of the
    /// city either: it starts after the name, or there is none (#502).
    /// Returns the range and whether the rule alone found where the city
    /// starts: after a body's name that is the match's `lead`, or after a
    /// stop's `body` word ("Review Board Washington, DC 20599"). Such a
    /// city stands though a model name runs into it ("Coastal Safety Board
    /// Wexmoor" of the model): the name ends before it (`correct`).
    fn guessed_city(
        &self,
        source: &str,
        (names, model): (&[Detection], &[std::ops::Range<usize>]),
        pattern: &Alternation,
        stop: Option<&Regex>,
        mut range: std::ops::Range<usize>,
        whole: &std::ops::Range<usize>,
    ) -> (std::ops::Range<usize>, bool) {
        // The match after `at`, within `end`, without its lead and tail.
        let again = |at: usize, end: usize| {
            let found = pattern.captures_at(source, at)?;
            let matched = found.get(0)?;
            let start = found
                .name("lead")
                .map_or(matched.start(), |lead| lead.end());
            let stop = found
                .name("tail")
                .map_or(matched.end(), |tail| tail.start());
            (start < stop && stop <= end).then_some(start..stop)
        };
        // Cut at each name and field after the city's start, searched again
        // after it; a recipient's label (Attn) is followed by a name, not a
        // city.
        let cut = |names: &[Detection], mut range: std::ops::Range<usize>| {
            while let Some((_, resume)) =
                guessed_cut(source, names, &range, range.start, false, stop)
            {
                match resume.and_then(|resume| again(resume, range.end)) {
                    Some(found) => range = found,
                    None => return range.start..range.start,
                }
            }
            range
        };
        let own = cut(&[], range.clone());
        let line = source[..own.start].rfind('\n').map_or(0, |at| at + 1);
        let before = source[line..own.start].trim_end_matches([' ', '\t', ',']);
        let body = stop.is_some_and(|stop| {
            stop.captures_iter(before).any(|found| {
                found.name("body").is_some()
                    && found.get(0).is_some_and(|word| word.end() == before.len())
            })
        });
        if !own.is_empty() && (own == range && whole.start < range.start || body) {
            return (cut(names, own), true);
        }
        if let Some(after) = model
            .iter()
            .filter(|name| {
                name.start < range.start && range.start < name.end && name.end < range.end
            })
            .map(|name| name.end)
            .max()
        {
            match again(after, range.end) {
                Some(found) => range = found,
                None => return (range.start..range.start, false),
            }
        }
        (cut(names, range), false)
    }

    /// Matches beside existing evidence, all in view coordinates. Definite
    /// street/building components may contain names; a guessed town tail
    /// never consumes an independent PERSON/ORG or a contact field. The
    /// `model` ranges are the model's people and companies as it gave them,
    /// those the fixes dropped too (a public body's name): a guessed city
    /// starts after one that starts before it and ends in it.
    pub(crate) fn spans_beside(
        &self,
        source: &str,
        names: &[Detection],
        model: &[std::ops::Range<usize>],
    ) -> Vec<SpanMatch> {
        self.spans
            .iter()
            .enumerate()
            .filter_map(|(entry, span)| match span {
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
                    (split, skip, *gaps_only, entry),
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
                    (split, skip, gaps_only, entry),
                    line_length,
                )| {
                    span_ranges_sized(pattern, source, line_length.as_ref())
                        .into_iter()
                        .map(move |(mut range, whole)| {
                            if let Some(after) = continuation_after.as_ref() {
                                let core = after.find(&source[range.clone()]);
                                // A match with no definite component (a
                                // city, state and ZIP alone) is guessed
                                // whole: its city takes no name or field
                                // after its start, and the pattern is
                                // searched again after them ("Ship To
                                // Ashby Falls, OH 45999"); a name at its
                                // front edge is `address_beside`'s.
                                if core.is_none() {
                                    let (range, city) = self.guessed_city(
                                        source,
                                        (names, model),
                                        pattern,
                                        continuation_stop.as_ref(),
                                        range,
                                        &whole,
                                    );
                                    return (range, whole, city);
                                }
                                let tail = range.start + core.map_or(0, |core| core.end());
                                // A name crossing the definite component's
                                // edge ("Broadway Show") invalidates the
                                // guessed street; never cut it to a fragment.
                                if names.iter().any(|name| {
                                    matches!(
                                        name.category,
                                        Category::Person | Category::Organization
                                    ) && name.range.start < tail
                                        && tail < name.range.end
                                        && range.start < name.range.end
                                }) {
                                    range.end = range.start;
                                    return (range, whole, false);
                                }
                                if let Some((end, _)) = guessed_cut(
                                    source,
                                    names,
                                    &range,
                                    tail,
                                    true,
                                    continuation_stop.as_ref(),
                                ) {
                                    range.end = range.start
                                        + source[range.start..end]
                                            .trim_end_matches(|c: char| {
                                                c.is_whitespace() || [',', ';', ':'].contains(&c)
                                            })
                                            .len();
                                    // Cutting before a recipient must not
                                    // promote a bare street ("Broadway")
                                    // or a house number alone ("1720"),
                                    // which the shape takes for a
                                    // postcode.
                                    let text = &source[range.clone()];
                                    let digits = text.trim_end_matches(char::is_alphabetic);
                                    let number_alone = !digits.is_empty()
                                        && digits.chars().all(|c| c.is_ascii_digit())
                                        && text[digits.len()..].chars().count() <= 1;
                                    if number_alone
                                        || !self.shaped_named_after(
                                            text,
                                            category,
                                            &|_| false,
                                            &source[..range.start],
                                        )
                                    {
                                        range.end = range.start;
                                    }
                                }
                            }
                            (range, whole, false)
                        })
                        // A split match's spans are the pieces between the
                        // split's matches ("KRENC<<LIDIIA" is two names); the
                        // whole match goes with each.
                        .flat_map(move |(range, whole, city)| match split {
                            Some(split) => pieces(split, source, range)
                                .into_iter()
                                .map(|piece| (piece, Some(whole.clone()), false))
                                .collect(),
                            None => vec![(range, None, city)],
                        })
                        .filter_map(move |(range, whole, city)| {
                            if category != Category::Address || range.is_empty() {
                                return Some((range, whole, city));
                            }
                            self.address_beside(source, names, (entry, pattern), range, city)
                                .map(|range| (range, whole, city))
                        })
                        // An address match before a colon ends in a field's
                        // label ("12 Oak Road, Email: …"), not the address's
                        // last part; a name before one is a speaker's ("ФИО1
                        // А. Б.: …").
                        .filter(move |(range, _, _)| {
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
                        .map(move |(range, whole, city)| SpanMatch {
                            detection: Detection {
                                range,
                                category,
                                score: CONTEXT_SCORE,
                                recognizer: Recognizer::Span,
                            },
                            whole,
                            entry,
                            city,
                        })
                },
            )
            .collect()
    }
}

/// Where the guessed part of an address match `range`, from `tail`, ends:
/// at the first person's or company's name (from `tail` on where
/// `at_tail`, after it otherwise) or the first `stop` field, with where a
/// search may go on after it: the end of that name or field, or none after
/// a field the `stop` regex's `recipient` group holds (Attn). A match of
/// its `place` group is none.
fn guessed_cut(
    source: &str,
    names: &[Detection],
    range: &std::ops::Range<usize>,
    tail: usize,
    at_tail: bool,
    stop: Option<&Regex>,
) -> Option<(usize, Option<usize>)> {
    let name = names
        .iter()
        .filter(|name| {
            matches!(name.category, Category::Person | Category::Organization)
                && (tail < name.range.start || (at_tail && tail == name.range.start))
                && name.range.start < range.end
        })
        .map(|name| (name.range.start, Some(name.range.end)))
        .min();
    // A `place` group is a real place with a stop word in its name
    // ("Council Bluffs"), no stop.
    let field = stop
        .and_then(|pattern| {
            pattern
                .captures_iter(&source[tail..range.end])
                .find(|found| found.name("place").is_none())
        })
        .map(|found| {
            let field = found.get(0).expect("group 0 is the match");
            let resume = found
                .name("recipient")
                .is_none()
                .then_some(tail + field.end());
            (tail + field.start(), resume)
        });
    name.into_iter()
        .chain(field)
        .min_by_key(|(start, _)| *start)
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
