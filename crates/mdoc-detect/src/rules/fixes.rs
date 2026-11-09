//! Span fixes: the `span` entries without a pattern, which grow, trim and
//! recategorize model spans, and `Pack::fix_spans`, which applies them with
//! every other rule on the model's spans.
use super::context::field_code;
use super::{Pack, Span, SpanFix};
use mdoc_pii::{Category, Detection};
use regex::Regex;
use std::sync::OnceLock;

/// The most bytes a company's name in nested quotes grows to
/// (`SpanFix::nested_names`).
const NESTED_NAME_MAX: usize = 1000;

/// A closed pair of quotes on one line, in the source: where its opening
/// quote starts, where its closing quote starts and where that ends.
#[derive(Clone, Copy, PartialEq)]
struct Quoted {
    open: usize,
    close: usize,
    end: usize,
}

impl Quoted {
    /// Whether this pair is inside `outer`, quotes and all.
    fn inside(&self, outer: &Quoted) -> bool {
        outer.open < self.open && self.end <= outer.close
    }
}

/// The closed pairs of quotes in `line`, at `at` of the source: «» and
/// straight quotes, nested. A straight quote opens after a space, the
/// line's start or a bracket, before what is no space, and closes the
/// innermost open straight quote after what is no space, so `"Группа
/// "Север""` is two pairs and `15" экран` none. A closing quote of the other
/// kind than the innermost open one leaves every open quote unclosed:
/// «Кедр "Север» closes nothing.
fn quote_pairs(line: &str, at: usize) -> Vec<Quoted> {
    let mut open: Vec<(usize, char)> = Vec::new();
    let mut pairs = Vec::new();
    let mut prev: Option<char> = None;
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let next = chars.peek().map(|&(_, next)| next);
        let opens = prev.is_none_or(|p| p.is_whitespace() || "([«".contains(p))
            && next.is_some_and(|n| !n.is_whitespace());
        let closes = prev.is_some_and(|p| !p.is_whitespace() && !"([".contains(p));
        let quote = match c {
            '«' => {
                open.push((i, c));
                None
            }
            '»' => Some('«'),
            '"' if closes && open.last().is_some_and(|&(_, q)| q == '"') => Some('"'),
            '"' if opens => {
                open.push((i, c));
                None
            }
            // A straight quote after a word with none open closes nothing
            // ("15\"").
            '"' if closes && open.iter().any(|&(_, q)| q == '"') => Some('"'),
            _ => None,
        };
        if let Some(quote) = quote {
            match open.last() {
                Some(&(start, opened)) if opened == quote => {
                    open.pop();
                    pairs.push(Quoted {
                        open: at + start,
                        close: at + i,
                        end: at + i + c.len_utf8(),
                    });
                }
                Some(_) => open.clear(),
                None => {}
            }
        }
        prev = Some(c);
    }
    pairs
}

impl SpanFix {
    /// A company's name in nested quotes is one span (guide rule 2,
    /// #557): «Правовая группа «Кедровый щит»», "Группа компаний "Северный
    /// мост"". Such a name is a pair of quotes on one line, inside one pair
    /// at most, whose last inner pair closes with it and holds no pair
    /// itself; its first letter is a capital or a digit, and no legal form
    /// is inside it, or it is a quotation that mentions companies («ООО
    /// «Кедр» и АО «Север»»). With one of the `nested_quotes` legal forms
    /// just before it, every span of the category on the form or in the
    /// quotes grows to the whole name, form and closing quotes included,
    /// and so becomes one. Without one, only pieces the quotes cut grow:
    /// spans inside the outer quotes with nothing but spaces and quotes
    /// between them and around them there, to the name inside its outer
    /// quotes, or with them when a span holds one. A growth never takes a
    /// span of another category, unless in the last inner pair («Фирма
    /// «Петров П. П.»»), nor markup, a table's cell border, text the view
    /// reads otherwise or more than `NESTED_NAME_MAX` bytes.
    fn nested_names(
        &self,
        source: &str,
        view: &mdoc_pii::view::View,
        detections: &mut Vec<Detection>,
    ) {
        /// The bytes before a name's outer quote its legal form is looked
        /// for in.
        const FORM_WINDOW: usize = 120;
        let mut lines: Vec<usize> = detections
            .iter()
            .filter(|d| d.category == self.category)
            .map(|d| source[..d.range.start].rfind('\n').map_or(0, |at| at + 1))
            .collect();
        lines.sort_unstable();
        lines.dedup();
        let mut gone = vec![false; detections.len()];
        let quote_or_space = |text: &str| {
            text.chars()
                .all(|c| c.is_whitespace() || matches!(c, '«' | '»' | '"'))
        };
        for start in lines {
            let end = source[start..]
                .find('\n')
                .map_or(source.len(), |at| start + at);
            let pairs = quote_pairs(&source[start..end], start);
            for name in &pairs {
                let mut window = name.open.saturating_sub(FORM_WINDOW).max(start);
                while !source.is_char_boundary(window) {
                    window += 1;
                }
                // Spans of the category on this line from `from` to the
                // name's end, and on no other quotes.
                let on = |i: usize, from: usize| {
                    let range = &detections[i].range;
                    !gone[i]
                        && detections[i].category == self.category
                        && start <= range.start
                        && range.end <= end
                        && range.start < name.end
                        && from < range.end
                        && !pairs.iter().any(|p| {
                            (p.end <= name.open || name.end <= p.open)
                                && p.open < range.end
                                && range.start < p.end
                        })
                };
                if !(0..detections.len()).any(|i| on(i, window)) {
                    continue;
                }
                let inner: Vec<&Quoted> = pairs.iter().filter(|p| p.inside(name)).collect();
                let Some(last) = inner.iter().find(|p| p.end == name.close) else {
                    continue;
                };
                if pairs.iter().filter(|outer| name.inside(outer)).count() > 1
                    || inner.iter().any(|p| inner.iter().any(|q| q.inside(p)))
                {
                    continue;
                }
                let open_end =
                    name.open + source[name.open..].chars().next().map_or(1, char::len_utf8);
                let interior = &source[open_end..name.close];
                if !interior
                    .chars()
                    .find(|c| c.is_alphanumeric())
                    .is_some_and(|c| c.is_uppercase() || c.is_numeric())
                    || self
                        .nested_quotes
                        .iter()
                        .any(|(_, anywhere)| anywhere.is_match(interior))
                {
                    continue;
                }
                // The window starts after a space, so it cuts no word.
                let before = &source[window..name.open];
                let before = if window == start {
                    before
                } else {
                    before
                        .find(char::is_whitespace)
                        .map_or("", |at| &before[at..])
                };
                let before_at = name.open - before.len();
                let form = self
                    .nested_quotes
                    .iter()
                    .filter_map(|(form, _)| form.find(before))
                    .map(|found| before_at + found.start())
                    .min();
                let from = form.unwrap_or(name.open);
                let mut touching: Vec<usize> =
                    (0..detections.len()).filter(|&i| on(i, from)).collect();
                touching.sort_by_key(|&i| detections[i].range.start);
                if touching.is_empty() {
                    continue;
                }
                let holds =
                    |range: &std::ops::Range<usize>, at: usize| range.start <= at && at < range.end;
                let quoted = form.is_some()
                    || touching.iter().any(|&i| {
                        let range = &detections[i].range;
                        holds(range, name.open) || holds(range, name.close)
                    });
                // Without a form, the spans are the name's pieces, cut at
                // its quotes.
                let pieces = || {
                    let mut at = open_end;
                    touching.iter().all(|&i| {
                        let range = &detections[i].range;
                        let gap = quote_or_space(&source[at.min(range.start)..range.start]);
                        at = at.max(range.end);
                        name.open <= range.start && range.end <= name.end && gap
                    }) && quote_or_space(&source[at.min(name.close)..name.close])
                };
                let cut = || {
                    let range = &detections[touching[0]].range;
                    std::iter::once(name)
                        .chain(inner.iter().copied())
                        .any(|p| holds(range, p.open) != holds(range, p.close))
                };
                if form.is_none() && !(pieces() && (touching.len() > 1 || quoted || cut())) {
                    continue;
                }
                let whole = if quoted {
                    from..name.end
                } else {
                    open_end..name.close
                };
                let grown = touching.iter().fold(whole, |grown, &i| {
                    let range = &detections[i].range;
                    grown.start.min(range.start)..grown.end.max(range.end)
                });
                // Markup (emphasis, a link, a tag), a table's cell border or
                // an entity the view decodes would leave the review no span.
                let seen = view.view_range(grown.clone());
                if grown.len() > NESTED_NAME_MAX
                    || source[grown.clone()]
                        .contains(['|', '*', '_', '~', '`', '[', ']', '<', '>', '\\'])
                    || view.text.get(seen) != source.get(grown.clone())
                    || detections.iter().enumerate().any(|(i, other)| {
                        other.category != self.category
                            && !gone[i]
                            && other.range.start < grown.end
                            && grown.start < other.range.end
                            && !(last.open < other.range.start && other.range.end <= last.close)
                    })
                {
                    continue;
                }
                // One span: the first keeps the best score.
                let first = touching[0];
                let score = touching
                    .iter()
                    .map(|&i| detections[i].score)
                    .fold(f32::MIN, f32::max);
                detections[first].range = grown;
                detections[first].score = score;
                for &i in &touching[1..] {
                    gone[i] = true;
                }
            }
        }
        let mut index = 0;
        detections.retain(|_| {
            index += 1;
            !gone[index - 1]
        });
    }

    /// `range` without its `trim` words or phrases at either edge (a word's
    /// final period or colon with it; a comma before a trailing one goes
    /// with it), its `trim_start` prefixes and its `strip_end` characters.
    /// An entry with `extend` patterns takes its `trim_start` prefixes off
    /// only a span it `grew`: "HON WAI KEUNG" from the model stays whole.
    fn trimmed(
        &self,
        source: &str,
        mut range: std::ops::Range<usize>,
        grew: bool,
    ) -> std::ops::Range<usize> {
        let trim_start = if self.extend.is_empty() || grew {
            self.trim_start.as_slice()
        } else {
            &[]
        };
        let plain = |word: &str| word.trim_end_matches(['.', ':', ',']).to_lowercase();
        let listed = |word: &str| self.trim.contains(&plain(word));
        let phrases: Vec<Vec<&str>> = self
            .trim
            .iter()
            .filter(|entry| entry.contains(' '))
            .map(|entry| entry.split(' ').collect())
            .collect();
        loop {
            let text = &source[range.clone()];
            let lead = text.len() - text.trim_start().len();
            let tail = text.len() - text.trim_end().len();
            range = range.start + lead..range.end - tail;
            let text = &source[range.clone()];
            if self.list_letter
                && let Some((marker, rest)) = text.split_once(char::is_whitespace)
                && let Some(letter) = marker
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                    .or_else(|| marker.strip_suffix(['.', ')']))
                && (letter.len() == 1 && letter.as_bytes()[0].is_ascii_lowercase()
                    || matches!(
                        letter,
                        "ii" | "iii" | "iv" | "vi" | "vii" | "viii" | "ix" | "xi" | "xii"
                    ))
            {
                let line = source[..range.start]
                    .rsplit('\n')
                    .next()
                    .unwrap_or_default();
                let before = line.trim_matches(|c: char| c.is_whitespace() || "#>*_|-".contains(c));
                if before.is_empty() || before.ends_with([':', ';']) {
                    range.start = range.end - rest.len();
                    continue;
                }
            }
            // A `rest` group is the name the prefix must leave: only what
            // is before it comes off.
            if let Some(end) = trim_start
                .iter()
                .filter_map(|pattern| {
                    let found = pattern.captures(text)?;
                    Some(
                        found
                            .name("rest")
                            .map_or(found.get(0)?.end(), |rest| rest.start()),
                    )
                })
                .find(|&end| 0 < end && end < text.len())
            {
                range.start += end;
                continue;
            }
            // A phrase at either edge, with a word left beside it: "In re
            // Quill" is "Quill".
            let words: Vec<(usize, &str)> = text
                .split(|c: char| c.is_whitespace() || [',', ':', '–', '—'].contains(&c))
                .filter(|word| !word.is_empty())
                .map(|word| (word.as_ptr() as usize - text.as_ptr() as usize, word))
                .collect();
            let is = |slice: &[(usize, &str)], phrase: &[&str]| {
                slice.len() == phrase.len()
                    && slice.windows(2).all(|pair| {
                        text[pair[0].0 + pair[0].1.len()..pair[1].0]
                            .chars()
                            .all(char::is_whitespace)
                    })
                    && slice
                        .iter()
                        .zip(phrase)
                        .all(|((_, word), listed)| plain(word) == *listed)
            };
            // An ORG role must be external to the name. Whitespace alone
            // cannot distinguish "Notary Public Underwriters, Inc".
            let after_role = |end: usize| {
                let rest = text[end..].trim_start();
                let separated = rest.trim_start_matches([',', ':', '-', '–', '—']);
                if separated.len() != rest.len() {
                    return Some(text.len() - separated.trim_start().len());
                }
                if rest.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("for"))
                    && rest[3..].starts_with(char::is_whitespace)
                {
                    let mut name = rest[3..].trim_start();
                    if let Some((first, rest)) = name.split_once(char::is_whitespace)
                        && ["the", "a", "an", "these", "those", "such"]
                            .contains(&first.to_lowercase().as_str())
                    {
                        name = rest.trim_start();
                    }
                    return Some(text.len() - name.len());
                }
                None
            };
            if let Some(phrase) = phrases
                .iter()
                .find(|phrase| words.len() > phrase.len() && is(&words[..phrase.len()], phrase))
            {
                let (_, last) = words[phrase.len() - 1];
                let end = words[phrase.len() - 1].0 + last.trim_end_matches([',', ':']).len();
                if !self.role_trim {
                    range.start += words[phrase.len()].0;
                    continue;
                } else if let Some(start) = after_role(end) {
                    range.start += start;
                    continue;
                }
            }
            if let Some(phrase) = phrases.iter().find(|phrase| {
                words.len() > phrase.len() && is(&words[words.len() - phrase.len()..], phrase)
            }) {
                let rest = &text[..words[words.len() - phrase.len()].0];
                if self.role_trim && !rest.trim_end().ends_with([',', ':', '-', '–', '—']) {
                    return range;
                }
                range.end = range.start
                    + rest
                        .trim_end_matches(|c: char| {
                            [',', ':', '-', '–', '—'].contains(&c) || c.is_whitespace()
                        })
                        .len();
                continue;
            }
            if let Some((first, rest)) = text.split_once(char::is_whitespace)
                && listed(first)
            {
                if !self.role_trim {
                    range.start = range.end - rest.len();
                    continue;
                } else if let Some(start) = after_role(first.trim_end_matches([',', ':']).len()) {
                    range.start += start;
                    continue;
                }
            }
            if let Some((rest, last)) = text.rsplit_once(char::is_whitespace)
                && listed(last)
                && (!self.role_trim || rest.trim_end().ends_with([',', ':', '-', '–', '—']))
            {
                range.end = range.start
                    + rest
                        .trim_end_matches(|c: char| {
                            c.is_whitespace() || [',', ':', '-', '–', '—'].contains(&c)
                        })
                        .len();
                continue;
            }
            if let Some(c) = text.chars().next_back()
                && text.len() > c.len_utf8()
                && self.strip_end.contains(c)
                && !(c == '.' && ends_with_initial(&text[..text.len() - 1]))
            {
                range.end -= c.len_utf8();
                continue;
            }
            return range;
        }
    }
}

/// Whether the person span at `range` of `source` is a list letter alone
/// at a line's start: one capital and its period, after list or quote
/// marks only, with a capitalized word or nothing after it on the line:
/// "J." of "J. Recommendations", "M." on a line of its own (guide rule 1,
/// issue #500). It is an initial where a name follows: a person span in
/// `people` (their starts) opens the next word, on its line or the next
/// ("Signed:\nJ. Smith"), or the line before ends in a label's colon
/// ("Witnesses:\nK. Ashdale"). A letter in a table's cell is no list's.
fn list_letter(source: &str, range: std::ops::Range<usize>, people: &[usize]) -> bool {
    let text = &source[range.clone()];
    let mut chars = text.chars();
    if !(chars.next().is_some_and(char::is_uppercase) && chars.as_str() == ".") {
        return false;
    }
    let line_start = source[..range.start].rfind('\n').map_or(0, |i| i + 1);
    if !source[line_start..range.start]
        .chars()
        .all(|c| matches!(c, ' ' | '\t' | '#' | '>' | '*' | '_'))
    {
        return false;
    }
    let rest = &source[range.end..];
    let line = rest.split('\n').next().unwrap_or_default();
    let after = line.trim_start_matches([' ', '\t']);
    let after = after.strip_suffix('\r').unwrap_or(after);
    if !(after.is_empty() || after.starts_with(char::is_uppercase) && line != after) {
        return false;
    }
    // The next word: on the line, or at the start of the next one.
    let gap = if after.is_empty() {
        let next = &rest[line.len()..];
        let next = next.strip_prefix('\n').unwrap_or(next);
        rest.len() - next.trim_start_matches([' ', '\t']).len()
    } else {
        line.len() - after.len()
    };
    if people.contains(&(range.end + gap)) {
        return false;
    }
    let label = source[..line_start]
        .strip_suffix('\n')
        .map(|before| before.strip_suffix('\r').unwrap_or(before))
        .and_then(|before| before.rsplit('\n').next())
        .is_some_and(|before| before.trim_end().ends_with(':'));
    !label
}

/// Whether `text` ends with a single capital letter on its own, an
/// initial whose period belongs to the name ("Белявская О. М", "Smith J").
fn ends_with_initial(text: &str) -> bool {
    let mut chars = text.chars().rev();
    chars.next().is_some_and(char::is_uppercase)
        && chars.next().is_none_or(|c| c.is_whitespace() || c == '.')
}

impl Pack {
    /// The `span` fixes on `detections` in `source`: each grows to an
    /// `extend` match it lies inside, loses `trim` words at either edge and
    /// `strip_end` characters at its end. A span left without its shape
    /// goes, read as `view` reads it, and so does one a cited case holds
    /// (`cited_cases`). Returns the precedents' ranges in `source`: cited
    /// cases naming no party of the case at hand.
    pub(crate) fn fix_spans(
        &self,
        source: &str,
        view: &mdoc_pii::view::View,
        detections: &mut Vec<Detection>,
    ) -> Vec<std::ops::Range<usize>> {
        static EMPLOYEE: OnceLock<Regex> = OnceLock::new();
        let employee = EMPLOYEE.get_or_init(|| {
            Regex::new(r"(?i)\b(?:сотрудник|офицер|инспектор)\p{Alphabetic}{0,3}[\p{Zs}\t]+$")
                .expect("employee affiliation pattern")
        });
        let fixes = self.spans.iter().filter_map(|span| match span {
            Span::Fix(fix) => Some(fix),
            Span::Pattern { .. } => None,
        });
        for fix in fixes {
            if !detections.iter().any(|d| d.category == fix.category) {
                continue;
            }
            if !fix.nested_quotes.is_empty() {
                fix.nested_names(source, view, detections);
            }
            // A `lead` group is context the grown span leaves out ("Judge "
            // before "K Ashdale"), and so is a `tail` group after it (the
            // year after "Penrow, J. L.", #414). The next match is searched
            // for from just after this one's grown start, so its lead may
            // be the end of this one ("K Ashdale and L Brown"). Each match
            // says whether it had a lead or a tail, and whether a tail.
            let matches: Vec<(std::ops::Range<usize>, bool, bool)> = fix
                .extend
                .iter()
                .flat_map(|pattern| {
                    if pattern.capture_names().flatten().next().is_none() {
                        return pattern
                            .find_iter(source)
                            .map(|m| (m.range(), false, false))
                            .collect();
                    }
                    let mut found = Vec::new();
                    let mut at = 0;
                    while at <= source.len()
                        && let Some(captures) = pattern.captures_at(source, at)
                    {
                        let Some(whole) = captures.get(0) else { break };
                        let start = captures
                            .name("lead")
                            .map_or(whole.start(), |lead| lead.end());
                        let end = captures
                            .name("tail")
                            .map_or(whole.end(), |tail| tail.start());
                        if start < end {
                            found.push((start..end, true, captures.name("tail").is_some()));
                        }
                        // Past this match's first grown character at least.
                        at = start + source[start..].chars().next().map_or(1, char::len_utf8);
                    }
                    found
                })
                .filter(|(m, _, _)| {
                    if !source[m.clone()].contains(['«', '"'])
                        || !self.public_body(&source[m.clone()], fix.category, false)
                    {
                        return true;
                    }
                    let start = source[..m.start]
                        .rfind(['\n', '.', ';'])
                        .map_or(0, |at| at + 1);
                    let before = &source[start..m.start];
                    // An inner agency quotation cannot overrule the
                    // private company enclosing it, in either category.
                    let enclosed = self.legal_form(before)
                        && (before.matches('«').count() > before.matches('»').count()
                            || before.matches('"').count() % 2 == 1);
                    // Even an explicit police department can be an
                    // employee's affiliation before a quoted nickname.
                    !enclosed && !employee.is_match(before)
                })
                .collect();
            // In reading order: a span never grows into part of another
            // span of its category, so in "Ветлин Н. О. Брюн Л. И." the
            // initials after "Ветлин" stay his.
            let mut order: Vec<usize> = (0..detections.len())
                .filter(|&i| detections[i].category == fix.category)
                .collect();
            order.sort_by_key(|&i| detections[i].range.start);
            for &i in &order {
                let range = detections[i].range.clone();
                // A match after a `lead` (a person's, #387) never grows
                // over any part of another category's span either: "ACME
                // LLC" stays the company's. Others may: a person's
                // institution grows over the model's organization in it.
                let crosses = |(m, led, _): &(std::ops::Range<usize>, bool, bool)| {
                    order.iter().any(|&j| {
                        let other = &detections[j].range;
                        j != i
                            && other.start < m.end
                            && m.start < other.end
                            && !(m.start <= other.start && other.end <= m.end)
                    }) || *led
                        && detections.iter().any(|other| {
                            other.category != fix.category
                                && other.range.start < m.end
                                && m.start < other.range.end
                        })
                };
                // A person never grows back over a role or title before
                // it ("Ответчик" in "Ответчик Н. О." for the model's "Н.
                // О."); after it such a word is a surname ("J. R. Clerk").
                let over_never = |m: &std::ops::Range<usize>, tailed: bool| {
                    (fix.category == Category::Person
                        && source[m.start..range.start]
                            .split(|c: char| !c.is_alphanumeric() && c != '-')
                            .any(|word| {
                                !word.is_empty()
                                    && (self.never(word)
                                        || fix.unless_over.contains(&word.to_lowercase()))
                            })
                        // A supported quoted institution may grow over
                        // its public head; extension patterns must first
                        // establish the institution, not an affiliation.
                        && !(source[m.clone()].contains(['«', '"'])
                            && self.public_body(&source[m.clone()], fix.category, false)))
                        // A match with a `tail` (a surname-first name,
                        // #414) never grows over a role or an `unless_over`
                        // word after the span either: "France" in "Paris,
                        // France", "President" in "Pellwright, President".
                        || tailed
                            && source[range.end..m.end]
                                .split(|c: char| !c.is_alphanumeric() && c != '-')
                                .any(|word| {
                                    !word.is_empty()
                                        && (self.never(word)
                                            || fix.unless_over.contains(&word.to_lowercase()))
                                })
                };
                // A match with a `tail` grows only a span that starts with
                // it, or the rest of a name whose start is a span: the
                // model's "Penrow" and "J. L." in "Penrow, J. L." are one
                // name, its "J.R." alone in "Thereafter, J.R., a minor"
                // never takes the word before the comma (#414).
                let starts = |m: &std::ops::Range<usize>, tailed: bool| {
                    !tailed
                        || range.start == m.start
                        || order.iter().any(|&j| {
                            let other = &detections[j].range;
                            j != i && other.start == m.start && other.end <= m.end
                        })
                };
                let grown = matches
                    .iter()
                    .filter(|found| {
                        let (m, _, tailed) = found;
                        m.start <= range.start
                            && range.end <= m.end
                            && !crosses(found)
                            && !over_never(m, *tailed)
                            && starts(m, *tailed)
                    })
                    .map(|(m, _, _)| m)
                    .max_by_key(|m| m.len())
                    .cloned()
                    .unwrap_or(range.clone());
                let trimmed = fix.trimmed(source, grown.clone(), grown != range);
                // A trim never leaves a company's legal form alone: "АО ТТН"
                // stays whole, "АЗС ООО «Бета-Ойл»" loses "АЗС".
                // Nor may a job phrase consume a titled person's whole
                // name: "Mr Team Leader" must not become just "Mr". The
                // title fix can then remove Mr and establish the name.
                detections[i].range = if trimmed != grown
                    && (self.only_legal_form(&source[trimmed.clone()])
                        || fix.category == Category::Person
                            && self.role_person_name(&source[grown.clone()])
                            && !self.role_person_name(&source[trimmed.clone()])
                        || fix.role_trim
                            && !self.identifying_named(
                                &source[trimmed.clone()],
                                Category::Organization,
                                &|_| false,
                                false,
                            )
                        || fix.category == Category::Person
                            && self.titled(source, trimmed.end)
                            && !self.titled(source, trimmed.start)
                            && !self.shaped_named(
                                &source[trimmed.clone()],
                                Category::Person,
                                &|_| false,
                            )) {
                    grown
                } else {
                    trimmed
                };
            }
        }
        self.person_documents(source, detections);
        // Before citations drop a precedent's parties: "Spiliada Maritime
        // Corp" there still makes "Spiliada" a company's short name.
        let named = self.company_nouns(source, view, detections);
        self.company_short_names(source, view, detections, &named);
        // Before citations, common-name evidence and propagation: a party
        // code must never become evidence that a real name was mentioned.
        for reference in &self.conditional_references {
            let code = |series: regex::Match<'_>| {
                let end = series.end()
                    + view.text[series.end()..]
                        .bytes()
                        .take_while(u8::is_ascii_digit)
                        .count();
                &view.text[series.start()..end]
            };
            let defined: std::collections::HashSet<&str> = reference
                .when
                .iter()
                .flat_map(|pattern| pattern.captures_iter(&view.text))
                .filter(|captures| {
                    let whole = captures.get(0).expect("group 0 is the match");
                    if !whole.as_str().ends_with(|c: char| c.is_ascii_digit()) {
                        return true;
                    }
                    // A role followed by a code may instead open a company
                    // name: "Claimant C1 Holdings Ltd, trading as C1". The
                    // whole continuation is checked, not just the next
                    // word; an uncertain name keeps its marks.
                    !self.company_continuation(&view.text[whole.end()..])
                })
                .filter_map(|captures| captures.name("series").map(code))
                .collect();
            let series: std::collections::HashSet<_> =
                defined.iter().map(|name| &name[..1]).collect();
            if series.is_empty() {
                continue;
            }
            struct Code<'a> {
                range: std::ops::Range<usize>,
                name: &'a str,
                established: bool,
                compound: bool,
            }
            let codes: Vec<_> = reference
                .pattern
                .captures_iter(&view.text)
                .map(|captures| {
                    let whole = captures.get(0).expect("group 0 is the match");
                    let start = captures.name("lead").map_or(whole.start(), |m| m.end());
                    let end = captures.name("tail").map_or(whole.end(), |m| m.start());
                    Code {
                        range: start..end.max(start),
                        name: code(captures.name("series").expect("checked at load")),
                        established: ["series", "series2"].into_iter().all(|group| {
                            captures
                                .name(group)
                                .is_none_or(|m| series.contains(m.as_str()))
                        }),
                        compound: captures.name("series2").is_some(),
                    }
                })
                .collect();
            // Join only code enumerations. Every member must have an
            // established series; an unknown series or real name keeps
            // the whole model span. Do this before looking for names so
            // "C1 and C2" is not evidence of a company's short form.
            let mut groups: Vec<std::ops::Range<usize>> = Vec::new();
            for (index, current) in codes.iter().enumerate() {
                if let Some(group) = groups.last_mut()
                    && let previous = &codes[group.end - 1]
                    && matches!(
                        view.text[previous.range.end..current.range.start].trim(),
                        "," | "and" | ", and" | ",and"
                    )
                {
                    group.end = index + 1;
                } else {
                    groups.push(index..index + 1);
                }
            }
            // A code that opens a kept name protects its short mentions
            // too (GUIDE rule 2), even if another party defines that code.
            let protected: std::collections::HashSet<_> = codes
                .iter()
                .enumerate()
                .filter(|(index, current)| {
                    let group = groups.iter().find(|g| g.contains(index)).unwrap();
                    self.company_continuation(&view.text[current.range.end..])
                        || detections.iter().any(|d| {
                            if !reference.categories.contains(&d.category) {
                                return false;
                            }
                            let range = view.view_range(d.range.clone());
                            current.range.start <= range.start
                                && range.start < current.range.end
                                && codes[group.end - 1].range.end < range.end
                        })
                })
                .map(|(_, current)| current.name)
                .collect();
            let ranges: Vec<_> = groups
                .into_iter()
                .filter(|group| {
                    codes[group.clone()]
                        .iter()
                        .all(|current| current.established && !protected.contains(current.name))
                        && (group.len() > 1
                            || codes[group.start].compound
                            || defined.contains(codes[group.start].name))
                })
                .map(|group| codes[group.start].range.start..codes[group.end - 1].range.end)
                .collect();
            detections.retain_mut(|d| {
                if !reference.categories.contains(&d.category) {
                    return true;
                }
                let range = view.view_range(d.range.clone());
                if ranges
                    .iter()
                    .any(|found| found.start <= range.start && range.end <= found.end)
                {
                    return false;
                }
                // A model may include the defining label with its real
                // name: "Mrs Penrow (C1)" must keep "Penrow" alone.
                if let Some(found) = ranges.iter().find(|found| {
                    range.start < found.start
                        && found.end == range.end
                        && view.text[(*found).clone()].starts_with('(')
                        && view.text[(*found).clone()].ends_with(')')
                }) {
                    let end = range.start + view.text[range.start..found.start].trim_end().len();
                    d.range.end = view.source_range(range.start..end).end;
                }
                !d.range.is_empty()
            });
        }
        let precedents = self.cited_cases(view, detections);
        let abbreviations = self.unnamed_abbreviations(&view.text);
        let defined = self.defined_terms(&view.text);
        let named = self.named_words(view, detections);
        // A name-part header establishes the cell's name even when the
        // spelling is also a role ("Trustee", "Должник"; #575), and so does
        // a name part's label before a value when a neighbouring name part's
        // label holds a name ("Given name: Edda / Surname: Trustee"; #601).
        // A lone "Full name: Claimant" is a form's placeholder.
        let mut name_cells = mdoc_pii::columns::name_cells(source);
        name_cells.extend(mdoc_pii::columns::backed_labelled_names(source, &|range| {
            let text = &view.text[view.view_range(range)];
            self.identifying_named(text, Category::Person, &|_| false, false)
        }));
        let people: Vec<usize> = detections
            .iter()
            .filter(|d| d.category == Category::Person)
            .map(|d| d.range.start)
            .collect();
        detections.retain(|d| {
            let text = &view.text[view.view_range(d.range.clone())];
            let titled = d.category == Category::Person && self.titled(source, d.range.start);
            !d.range.is_empty()
                && !(d.category == Category::Person
                    && (list_letter(source, d.range.clone(), &people)
                        || self.programme_title(text, titled)
                            && !self.last_word_named(source, text, &named)))
                && !(d.category == Category::Person
                    && self.determined_role(source, d.range.clone()))
                && !(matches!(d.category, Category::Organization | Category::Person)
                    && self.cut_from_public_body(
                        &view.text,
                        view.view_range(d.range.clone()),
                        d.category,
                        titled,
                    ))
                && self.identifying_named_after(
                    text,
                    d.category,
                    &|word| {
                        d.category == Category::Person
                            && (titled || name_cells.contains(&d.range) || named.contains(word))
                    },
                    titled,
                    &view.text[..view.view_range(d.range.clone()).start],
                )
                && !field_code(source, d)
                && !(d.category == Category::Organization
                    && !defined.is_empty()
                    && self.defined_generic(text, &defined))
                && !(matches!(d.category, Category::Organization | Category::Person)
                    && abbreviations.contains(
                        text.trim_matches(|c: char| !c.is_alphanumeric())
                            .trim_end_matches("’s")
                            .trim_end_matches("'s"),
                    ))
        });
        precedents
    }
}
