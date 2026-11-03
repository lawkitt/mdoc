//! Immutable, cancellable candidate discovery; reusable cached matcher.
use super::{
    exact_boundary, generated_alias, interval_conflict,
    propagation::{glued, sentence_start},
    syntax::{crosses_syntax, plain_text_span, protected_syntax},
};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use std::{
    collections::{BTreeMap, HashSet},
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub struct DiscoveryInput {
    pub(super) source: Arc<str>,
    pub(super) version: u64,
    pub(super) originals: Vec<Arc<str>>,
    pub(super) enabled: Vec<bool>,
    pub(super) priority: Vec<u64>,
    pub(super) mid_sentence: Vec<bool>,
    /// Propagated: never inside an identifier, a path or an address.
    pub(super) unglued: Vec<bool>,
    /// Case numbers: a repeat counts only standing alone, never inside a
    /// longer identifier or number, nor as an amount ([`whole_value`]).
    pub(super) whole: Vec<bool>,
    /// Web addresses (ADR 0094): a mention is a whole address
    /// ([`crate::web::whole`]), and one shields the names, emails and IDs inside it
    /// even when kept.
    pub(super) url: Vec<bool>,
    /// URLs and emails match exact source bytes only, never decoded text.
    pub(super) raw_only: Vec<bool>,
    /// Dates of birth: a mention counts only after one of its cue words
    /// ([`cue_before`]); `None` counts every mention.
    pub(super) cues: Vec<Option<Vec<String>>>,
    pub(super) excluded: HashSet<(usize, usize)>,
    pub(super) matcher: Option<Arc<AhoCorasick>>,
}
pub struct DiscoveryResult {
    pub(super) version: u64,
    pub(super) mentions: Vec<Vec<Range<usize>>>,
    pub(super) matcher: Option<Arc<AhoCorasick>>,
    pub(super) tokens: HashSet<String>,
}
impl DiscoveryInput {
    pub fn run(self, cancel: &AtomicBool) -> Option<DiscoveryResult> {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let source: &str = &self.source;
        let tokens = source
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .filter(|v| generated_alias(v))
            .map(str::to_owned)
            .collect();
        let mut mentions = vec![Vec::new(); self.originals.len()];
        if self.originals.is_empty() {
            return Some(DiscoveryResult {
                version: self.version,
                mentions,
                matcher: None,
                tokens,
            });
        }
        let originals: Vec<_> = self
            .originals
            .iter()
            .zip(&self.raw_only)
            .map(|(s, raw_only)| {
                if *raw_only {
                    std::borrow::Cow::Borrowed(s.as_ref())
                } else {
                    crate::view::wording(s)
                }
            })
            .collect();
        // Two patterns per variant: its exact source and its read wording.
        // URL and email patterns stay raw, and only search the raw source.
        let matcher = self.matcher.unwrap_or_else(|| {
            Arc::new(
                AhoCorasickBuilder::new()
                    .kind(Some(AhoCorasickKind::ContiguousNFA))
                    .build(
                        self.originals
                            .iter()
                            .zip(&originals)
                            .flat_map(|(raw, read)| [raw.as_ref(), read.as_ref()]),
                    )
                    .expect("bounded nonempty seeds"),
            )
        });
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let protected = protected_syntax(source);
        let view = crate::view::View::for_discovery(source);
        let mut hits = Vec::new();
        // Prefer the exact source variant when it is already known, so a
        // kept or explicitly separated wording keeps its own policy.
        let exact: std::collections::HashMap<&str, usize> = self
            .originals
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_ref(), i))
            .collect();
        let raw = matcher
            .find_overlapping_iter(source)
            .map(|hit| (hit.pattern().as_usize() / 2, hit.start()..hit.end()));
        let decoded = matcher.find_overlapping_iter(&view.text).filter_map(|hit| {
            let pattern = hit.pattern().as_usize() / 2;
            if self.raw_only[pattern] {
                return None;
            }
            Some((pattern, view.source_range(hit.start()..hit.end())))
        });
        let mut seen = HashSet::new();
        for (index, (pattern, range)) in raw.chain(decoded).enumerate() {
            if index % 1024 == 0 && cancel.load(Ordering::Relaxed) {
                return None;
            }
            if range.is_empty() {
                continue;
            }
            let group = exact
                .get(&source[range.clone()])
                .copied()
                .unwrap_or(pattern);
            if seen.insert((group, range.start, range.end)) {
                hits.push((group, range));
            }
        }
        // The user's own selection (a manual mark) counts every exact
        // match; only a scan's repeats must be whole addresses and stay out
        // of a kept address.
        let manual = |index: usize| self.priority[index] > 0;
        let url_hit = |index: usize, range: &Range<usize>| {
            self.url[index]
                && exact_boundary(source, range, &self.originals[index])
                && (manual(index) || crate::web::whole(source, range))
        };
        // A web address, proposed, kept or replaced, holds what is inside it.
        let mut shields: Vec<Range<usize>> = hits
            .iter()
            .filter(|(index, range)| url_hit(*index, range))
            .map(|(_, range)| range.clone())
            .collect();
        // Sorted by start, longest first, with the furthest end so far: a
        // range is inside another shield when one starting before it
        // reaches its end, or a longer one starts with it. O(log n) each.
        shields.sort_by_key(|shield| (shield.start, std::cmp::Reverse(shield.end)));
        shields.dedup();
        let reach: Vec<usize> = shields
            .iter()
            .scan(0, |furthest, shield| {
                *furthest = shield.end.max(*furthest);
                Some(*furthest)
            })
            .collect();
        let shielded = |range: &Range<usize>| {
            let before = shields.partition_point(|shield| shield.start < range.start);
            before.checked_sub(1).is_some_and(|i| reach[i] >= range.end)
                || shields
                    .get(before)
                    .is_some_and(|shield| shield.start == range.start && shield.end > range.end)
        };
        hits.retain(|(index, range)| {
            self.enabled[*index]
                && !self.excluded.contains(&(range.start, range.end))
                && exact_boundary(source, range, &self.originals[*index])
                && (source[range.clone()] == *self.originals[*index] || !glued(source, range))
                && (!self.url[*index] || manual(*index) || crate::web::whole(source, range))
                && (manual(*index) || !shielded(range))
                && !crosses_syntax(&protected, &view.markup, source, range)
                && plain_text_span(source, &view.markup, range.clone())
                && !(self.mid_sentence[*index] && sentence_start(source, range.start))
                && !(self.unglued[*index] && glued(source, range))
                && !(self.whole[*index] && !whole_value(source, range))
                && self.cues[*index]
                    .as_ref()
                    .is_none_or(|cues| cues.contains(&cue_before(source, range.start)))
        });
        // A hit inside a longer one never splits it, whatever its priority: a
        // manual "Raman" leaves "Priya Raman" whole (Same as… may link them).
        // Sorted by start then longest first, an earlier hit reaching as far
        // contains it; distinct originals never share a range.
        hits.sort_by_key(|(_, range)| (range.start, std::cmp::Reverse(range.end)));
        let mut reach = 0;
        hits.retain(|(_, range)| {
            let contained = reach >= range.end;
            reach = reach.max(range.end);
            !contained
        });
        // Higher-priority (manual) originals claim partial overlaps first.
        hits.sort_by_key(|(group, range)| {
            (
                std::cmp::Reverse(self.priority[*group]),
                *group,
                range.start,
            )
        });
        let mut occupied = BTreeMap::new();
        let mut last_end = 0;
        let single = self.originals.len() == 1;
        for (index, range) in hits {
            let conflict = if single {
                range.start < last_end
            } else {
                interval_conflict(&occupied, &range)
            };
            if conflict {
                continue;
            }
            if !single {
                occupied.insert(range.start, range.end);
            }
            last_end = range.end;
            mentions[index].push(range);
        }
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        Some(DiscoveryResult {
            version: self.version,
            mentions,
            matcher: Some(matcher),
            tokens,
        })
    }
}

/// What labels a date of birth at `start`, lowercase (#466): the header of
/// its table column ("| Name | DOB |"), else the label just before it. A
/// neighbouring value takes no part, so replacing a name beside the date
/// keeps it. Empty when there is none.
pub(crate) fn cue_before(source: &str, start: usize) -> String {
    column_header(source, start).unwrap_or_else(|| label_before(source, start))
}

/// The label before `start` ("DOB: ", "род. "), across spaces, one line
/// break and up to 12 other marks: its last word, and the word before when
/// that is lowercase, the last is short, or both are capitalised alike. So
/// "born on" is not "Signed on", "BIRTH DATE" is not "SIGNING DATE", and
/// "Bennett born" is "born".
fn label_before(source: &str, start: usize) -> String {
    let before = &source[..start];
    let (mut marks, mut breaks, mut end) = (0, 0, None);
    for (i, c) in before.char_indices().rev() {
        if c.is_alphanumeric() {
            end = Some(i + c.len_utf8());
            break;
        }
        if c == '\n' {
            breaks += 1;
        } else if !c.is_whitespace() {
            marks += 1;
        }
        if breaks > 1 || marks > 12 {
            break;
        }
    }
    let Some(end) = end else {
        return String::new();
    };
    let rest = before[..end].trim_end_matches(char::is_alphanumeric);
    let word = &before[rest.len()..end];
    let Some(rest) = rest.strip_suffix([' ', '\t']) else {
        return word.to_lowercase();
    };
    let previous = &rest[rest.trim_end_matches(char::is_alphanumeric).len()..];
    // 0: lowercase, 1: capitalised, 2: capitals.
    let shape = |w: &str| {
        if !w.chars().any(char::is_lowercase) {
            2
        } else if w.starts_with(char::is_uppercase) {
            1
        } else {
            0
        }
    };
    let joins = !previous.is_empty()
        && previous.chars().all(char::is_alphabetic)
        && (shape(previous) == 0 || shape(previous) == shape(word) || word.chars().count() <= 2);
    if joins {
        format!("{previous} {word}").to_lowercase()
    } else {
        word.to_lowercase()
    }
}

/// Byte offsets of a table row's cell pipes; an escaped `\|` is text.
fn pipes(row: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut escaped = false;
    for (i, c) in row.char_indices() {
        if c == '|' && !escaped {
            found.push(i);
        }
        escaped = c == '\\' && !escaped;
    }
    found
}

/// The header cell of the table column `start` is in: up the table's rows
/// to the one a delimiter row ("| --- |") follows.
fn column_header(source: &str, start: usize) -> Option<String> {
    let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
    if !source[line_start..].trim_start().starts_with('|') {
        return None;
    }
    let column = pipes(&source[line_start..start]).len();
    let mut rows = source[..line_start].lines().rev();
    loop {
        let row = rows.next()?.trim();
        if !row.starts_with('|') {
            return None;
        }
        if row.contains('-')
            && row
                .chars()
                .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
        {
            let header = rows.next()?;
            let cells = pipes(header);
            let from = cells.get(column.checked_sub(1)?)? + 1;
            let label = header[from..cells.get(column).copied().unwrap_or(header.len())]
                .trim_matches(|c: char| c == '*' || c == '_' || c == ':' || c.is_whitespace())
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            return (!label.is_empty()).then_some(label);
        }
    }
}

/// Whether `range` stands alone as a value: not joined to a longer
/// identifier ("2-7316/2026") or a digit group ("14 632"), and not an amount
/// ("7316 рублей", "£7316"; the fixture guide never marks amounts). A case
/// number's repeats count only so: its digits elsewhere are other numbers.
pub(crate) fn whole_value(source: &str, range: &Range<usize>) -> bool {
    // Amounts' units and currencies, lowercase, as they start or end.
    const AFTER: [&str; 22] = [
        "руб",
        "₽",
        "коп",
        "долл",
        "евро",
        "тыс",
        "млн",
        "млрд",
        "%",
        "процент",
        "usd",
        "eur",
        "gbp",
        "rub",
        "$",
        "€",
        "£",
        "pound",
        "dollar",
        "euro",
        "percent",
        "per cent",
    ];
    const BEFORE: [&str; 8] = ["$", "€", "£", "₽", "usd", "eur", "gbp", "rub"];
    let value = &source[range.clone()];
    let mut back = source[..range.start].chars().rev();
    let (b1, b2) = (back.next(), back.next());
    let mut ahead = source[range.end..].chars();
    let (a1, a2) = (ahead.next(), ahead.next());
    let joined = |separator: Option<char>, next: Option<char>| {
        separator.is_some_and(|c| r"-/.:_\@".contains(c)) && next.is_some_and(char::is_alphanumeric)
    };
    let grouped = |space: Option<char>, digit: Option<char>| {
        space.is_some_and(|c| matches!(c, ' ' | '\u{a0}' | '\u{202f}'))
            && digit.is_some_and(|c| c.is_ascii_digit())
    };
    if joined(b1, b2)
        || joined(a1, a2)
        || (value.starts_with(|c: char| c.is_ascii_digit()) && grouped(b1, b2))
        || (value.ends_with(|c: char| c.is_ascii_digit()) && grouped(a1, a2))
    {
        return false;
    }
    let near = |text: &str, from_start: bool| {
        let text = if from_start {
            &text[..text.floor_char_boundary(text.len().min(16))]
        } else {
            &text[text.floor_char_boundary(text.len().saturating_sub(16))..]
        };
        text.to_lowercase()
    };
    let after = near(source[range.end..].trim_start(), true);
    let before = near(source[..range.start].trim_end(), false);
    !AFTER.iter().any(|unit| after.starts_with(unit))
        && !BEFORE.iter().any(|unit| before.ends_with(unit))
}

#[cfg(test)]
mod tests {
    use super::{cue_before, whole_value};

    #[test]
    fn a_cue_is_a_dates_column_header_or_label() {
        let table = "| Name | DOB |\n| --- | --- |\n| Maya Bennett | 14/03/1988 |";
        let escaped = "| Note | DOB |\n| --- | --- |\n| A \\| B | 14/03/1988 |";
        let longer = "| Name | **Дата рождения** |\n|-|-|\n| x | y |\n| PERSON_1 | 14.03.1988 |";
        for (source, date, cue) in [
            ("DOB: 14/03/1988", "14/03/1988", "dob"),
            ("| Дата рождения | 14.03.1988", "14.03.1988", "рождения"),
            ("Иванов И. И., род. 14.03.1988", "14.03.1988", "род"),
            ("DOB:\n14/03/1988", "14/03/1988", "dob"),
            ("Signed 14/03/1988", "14/03/1988", "signed"),
            ("born on 2 May 1990", "2 May", "born on"),
            ("Born on 2 May 1990", "2 May", "born on"),
            ("Signed on 2 May 1990", "2 May", "signed on"),
            ("Maya Bennett born 14 March 1988", "14 March", "born"),
            ("Date of birth: 14/03/1988", "14/03/1988", "of birth"),
            (table, "14/03/1988", "dob"),
            (longer, "14.03.1988", "дата рождения"),
            ("| DOB | 14/03/1988 |", "14/03/1988", "dob"),
            (escaped, "14/03/1988", "dob"),
            ("BIRTH DATE: 14/03/1988", "14/03/1988", "birth date"),
            ("SIGNING DATE: 14/03/1988", "14/03/1988", "signing date"),
            ("Maya Bennett DOB 14/03/1988", "14/03/1988", "dob"),
            ("PERSON_1 DOB 14/03/1988", "14/03/1988", "dob"),
            ("DOB:\n            14/03/1988", "14/03/1988", "dob"),
            ("14/03/1988", "14/03/1988", ""),
            ("x ------------- 14/03/1988", "14/03/1988", ""),
        ] {
            assert_eq!(
                cue_before(source, source.rfind(date).unwrap()),
                cue,
                "{source}"
            );
        }
    }

    #[test]
    fn a_value_stands_alone_only_outside_identifiers_numbers_and_amounts() {
        let at = |source: &str, value: &str| {
            let start = source.rfind(value).unwrap();
            whole_value(source, &(start..start + value.len()))
        };
        for (source, value) in [
            ("Номер дела: 7316.", "7316"),
            ("see case 7316, filed", "7316"),
            ("| 7316 |", "7316"),
        ] {
            assert!(at(source, value), "{source}");
        }
        for (source, value) in [
            ("| Возвращено частично | 7316 рублей |", "7316"),
            ("| 4821 рубль |", "4821"),
            ("paid £7316 in full", "7316"),
            ("paid GBP 7316", "7316"),
            ("7316 ₽", "7316"),
            ("дело 2-7316/2026", "7316"),
            ("14 7316 руб.", "7316"),
            ("total 14 7316", "7316"),
            ("ref 7316_a", "7316"),
        ] {
            assert!(!at(source, value), "{source}");
        }
    }
}
