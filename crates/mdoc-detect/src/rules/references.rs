//! Legal references and cited cases: a model span inside a reference is
//! never proposed, and one inside a cited case only where it names a party
//! of the case at hand.
use super::Pack;
use super::load::is_party;
use super::spans::span_ranges;
use mdoc_pii::{Category, Detection};
use regex::Regex;
use std::sync::OnceLock;

/// A cited case in the view's text: its range without `lead` and `tail`,
/// its parties' ranges, and whether a docket number cites it (its `docket`
/// group), which a caption may carry too.
pub(super) struct Cited {
    pub(super) range: std::ops::Range<usize>,
    parties: Vec<std::ops::Range<usize>>,
    docket: bool,
}

/// Where `at` in `text` starts once the emphasis just before it is left
/// out: "Mr *Pellam*" reads as "Mr Pellam".
fn unemphasised(text: &str, at: usize) -> usize {
    text[..at].trim_end_matches(['*', '_']).len()
}

impl Pack {
    /// A precedent's parties before a full or reporter short citation are
    /// legal references. Names outside citations, beside a role or title,
    /// or in the caption protect the case at hand's parties everywhere.
    /// The caption is the first accepted case introducer standing alone
    /// or introducing a judgment, even with a wrapped reporter citation.
    /// A needless mark costs one untick, so uncertain names stay (#561).
    pub(super) fn cited_cases(
        &self,
        view: &mdoc_pii::view::View,
        detections: &mut Vec<Detection>,
    ) -> Vec<std::ops::Range<usize>> {
        let cited = self.cited(&view.text);
        if cited.is_empty() {
            return Vec::new();
        }
        let text = &view.text;
        let words = |range: std::ops::Range<usize>| -> Vec<String> {
            text[range]
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_lowercase)
                .collect()
        };
        // Whether `outer` holds `inner`'s words in a row.
        let holds = |outer: &[String], inner: &[String]| {
            !inner.is_empty() && outer.windows(inner.len()).any(|run| run == inner)
        };
        let named = |d: &Detection| matches!(d.category, Category::Person | Category::Organization);
        let ranges: Vec<std::ops::Range<usize>> = detections
            .iter()
            .map(|d| view.view_range(d.range.clone()))
            .collect();
        let holder = |range: &std::ops::Range<usize>| {
            cited
                .iter()
                .position(|case| case.range.start <= range.start && range.end <= case.range.end)
        };
        // The caption's lines, including the whole matched citation when
        // either party or its reporter wraps.
        static CAPTION: OnceLock<Regex> = OnceLock::new();
        let mut own: Vec<Vec<String>> = CAPTION
            .get_or_init(|| {
                // New York's "-against-" too, on a line of its own.
                Regex::new(
                    r"(?:^|[\s|*_])(?P<caption>(?:v|vs)\.?|[Ii]n[\p{Zs}\t]+re|[Ee]x[\p{Zs}\t]+parte|(?:[Ii]n[\p{Zs}\t]+the[\p{Zs}\t]+)?[Mm]atter[\p{Zs}\t]+of)(?:[\s|*_]|$)|(?m:^)[\p{Zs}\t|*_]*(?P<against>-?(?:against|AGAINST)-?)[\p{Zs}\t|*_]*(?m:$)",
                )
                .expect("a pattern")
            })
            .captures_iter(text)
            .filter_map(|found| found.name("caption").or_else(|| found.name("against")))
            // A docket citation is a caption only where it opens its line
            // ("Maren Quill v. Branwick Freight LLC, Index No. …"), never
            // one cited in a sentence ("relies on Grant v. Finecy, Index
            // No. 650001/2030 (Sup. Ct. N.Y. 2030)") or after a signal.
            .find(|found| {
                holder(&found.range()).is_none_or(|case| {
                    cited[case].docket && opens_line(text, cited[case].range.start)
                        || cited_caption(text, &cited[case])
                })
            })
            .map(|found| {
                let mut start = text[..found.start()].rfind('\n').map_or(0, |i| i + 1);
                let mut end = text[found.end()..]
                    .find('\n')
                    .map_or(text.len(), |i| found.end() + i);
                if text[start..end].trim_matches(|c: char| c.is_whitespace() || "#>*_|".contains(c))
                    == found.as_str()
                {
                    start = text[..start]
                        .trim_end_matches(['\r', '\n'])
                        .rfind('\n')
                        .map_or(0, |i| i + 1);
                    if end < text.len() {
                        end = text[end + 1..]
                            .find('\n')
                            .map_or(text.len(), |i| end + 1 + i);
                    }
                }
                if let Some(case) = holder(&found.range()) {
                    start = start.min(cited[case].range.start);
                    end = end.max(cited[case].range.end);
                }
                words(start..end)
            })
            .into_iter()
            .collect();
        for case in &cited {
            let before = &text[..case.range.start];
            let start = before
                .char_indices()
                .rev()
                .take(60)
                .last()
                .map_or(0, |(at, _)| at);
            let context = before[start..].to_lowercase();
            if self
                .own_before
                .iter()
                .any(|phrase| context.contains(phrase))
            {
                own.extend(case.parties.iter().map(|party| words(party.clone())));
            }
        }
        let mut doubtful = vec![false; detections.len()];
        for (i, d) in detections.iter().enumerate() {
            let range = ranges[i].clone();
            let beside = named(d)
                && (self.beside_role(text, range.clone())
                    || self.titled(text, unemphasised(text, range.start)));
            // A permissive party pattern must never swallow an attribution
            // after a model name ("Ivo Harrow for Pellam"). Keep the name
            // even if a rule pack later admits such prose again.
            let prefix = named(d)
                && holder(&range).is_some_and(|case| {
                    cited[case].parties.iter().any(|party| {
                        range.start == party.start
                            && range.end < party.end
                            && text[range.end..party.end]
                                .split(|c: char| !c.is_alphabetic())
                                .any(|word| {
                                    !word.is_empty()
                                        && word.chars().all(char::is_lowercase)
                                        && !party_connector(word)
                                })
                    })
                });
            if holder(&range).is_some() && !beside && !prefix {
                doubtful[i] = true;
            } else if named(d) {
                own.push(words(range));
            }
        }
        let ours = |name: &[String]| {
            own.iter()
                .any(|other| holds(other, name) || holds(name, other))
        };
        // A cited case naming a party of the case at hand is its own earlier
        // decision, whose docket number is the case at hand's too.
        let earlier: Vec<bool> = cited
            .iter()
            .map(|case| case.parties.iter().any(|party| ours(&words(party.clone()))))
            .collect();
        // A related proceeding of the parties cited by its docket alone
        // keeps every name it holds, its other party's too ("Brendt
        // Holloway v. Harrow, Adv. Proc. No. 31-01992 (Bankr. S.D.N.Y.
        // 2031)"): it is no precedent. A reported decision's other party
        // is a reference, as in any precedent.
        let mut i = 0;
        detections.retain(|d| {
            let range = &ranges[i];
            let related = holder(range).is_some_and(|case| earlier[case]);
            let keep = !doubtful[i]
                || if named(d) {
                    ours(&words(range.clone()))
                        || related && holder(range).is_some_and(|case| cited[case].docket)
                } else {
                    related
                };
            i += 1;
            keep
        });
        cited
            .iter()
            .zip(&earlier)
            .filter(|(_, earlier)| !**earlier)
            .map(|(case, _)| view.source_range(case.range.clone()))
            .collect()
    }

    /// The cited cases in `text`, each `cases` pattern's matches.
    pub(super) fn cited(&self, text: &str) -> Vec<Cited> {
        let mut cited = Vec::new();
        for pattern in &self.cases {
            for found in pattern.captures_iter(text) {
                let whole = found.get(0).expect("group 0 is the match");
                let mut start = found.name("lead").map_or(whole.start(), |lead| lead.end());
                let end = found.name("tail").map_or(whole.end(), |tail| tail.start());
                let mut parties: Vec<_> = pattern
                    .capture_names()
                    .flatten()
                    .filter(|group| is_party(group))
                    .filter_map(|group| found.name(group))
                    .map(|party| party.range())
                    .collect();
                let mut uncertain = false;
                for party in &mut parties {
                    // A party may have consumed a sentence's final
                    // abbreviation ("Ivo Harrow Jr."). A period before
                    // another capitalised word is an uncertain boundary.
                    let mut after_boundary = party.start;
                    for (at, _) in text[party.clone()].match_indices('.') {
                        let rest = &text[party.start + at + 1..party.end];
                        let next = rest.trim_start_matches(char::is_whitespace);
                        if next.len() < rest.len()
                            && next.chars().next().is_some_and(char::is_uppercase)
                        {
                            after_boundary = party.end - next.len();
                        }
                    }
                    if after_boundary > party.start {
                        if party.start == start {
                            start = after_boundary;
                            party.start = start;
                        } else {
                            // A boundary after "v." or "In re" breaks the
                            // citation's shape: keep all its names.
                            uncertain = true;
                            break;
                        }
                    }
                }
                if !uncertain && start < end {
                    cited.push(Cited {
                        range: start..end,
                        parties,
                        docket: found.name("docket").is_some(),
                    });
                }
            }
        }
        cited
    }

    /// The lowercase word just before `at` in `text`, emphasis and the
    /// punctuation after it skipped, and where it starts.
    pub(super) fn word_before(text: &str, at: usize) -> (String, usize) {
        let before = text[..unemphasised(text, at)]
            .trim_end()
            .trim_end_matches([',', ':', '.', '*', '_'])
            .trim_end();
        let start = before.rfind(|c: char| !c.is_alphanumeric()).map_or(0, |i| {
            i + before[i..].chars().next().map_or(1, char::len_utf8)
        });
        (before[start..].to_lowercase(), start)
    }

    /// Whether a party's role ("Plaintiff", "Defendants") stands beside
    /// `range` of `text`: before it, a person's title between them or not
    /// ("Plaintiff Ivo Harrow", "the Defendant, Mr *Quill*"), or after it
    /// ("IVO HARROW, Plaintiff", "Quillon Ltd (Defendant)").
    pub(super) fn beside_role(&self, text: &str, range: std::ops::Range<usize>) -> bool {
        let role = |word: &str| {
            self.party_roles.contains(word)
                || word
                    .strip_suffix('s')
                    .is_some_and(|single| self.party_roles.contains(single))
        };
        let after = text[range.end..].trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, ',' | '(' | '-' | '–' | '*' | '_')
        });
        let next = after
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if role(&next) {
            return true;
        }
        let mut at = range.start;
        for _ in 0..2 {
            let (word, start) = Self::word_before(text, at);
            if role(&word) {
                return true;
            }
            if word.is_empty() || !self.titled(text, unemphasised(text, at)) {
                return false;
            }
            at = start;
        }
        false
    }

    /// Whether `range` of `source` lies inside a `reference` match, a legal
    /// reference ("424 U.S. at 38", "статьи 431 ГК РФ"): no identifier,
    /// address or name (guide rule 8). The span's lines are searched as
    /// detection reads them ("&sect;" is "§"), and again with the line on
    /// each side joined where a citation may wrap onto it ("904 F.2d\n275,
    /// 277"): a soft break between two lines of running text.
    pub(super) fn in_reference(&self, source: &str, range: std::ops::Range<usize>) -> bool {
        if self.references.is_empty() {
            return false;
        }
        let line_start = |at: usize| source[..at].rfind('\n').map_or(0, |i| i + 1);
        let line_end = |at: usize| source[at..].find('\n').map_or(source.len(), |i| at + i);
        let start = line_start(range.start);
        let end = line_end(range.end);
        let inside = |from: usize, text: &str| {
            let view = mdoc_pii::view::View::new(text);
            let span = view.view_range(range.start - from..range.end - from);
            self.references.iter().any(|pattern| {
                span_ranges(pattern, &view.text)
                    .into_iter()
                    .any(|found| found.start <= span.start && span.end <= found.end)
            })
        };
        if inside(start, &source[start..end]) {
            return true;
        }
        let mut wide = start..end;
        if start > 0 {
            let before = line_start(start - 1);
            if soft_break(&source[before..start - 1], &source[start..end]) {
                wide.start = before;
            }
        }
        if end < source.len() {
            let after = line_end(end + 1);
            if soft_break(&source[line_start(end)..end], &source[end + 1..after]) {
                wide.end = after;
            }
        }
        // One space for each line break: offsets stay the source's.
        let lines = &source[wide.clone()];
        lines.contains('\n') && inside(wide.start, &lines.replace(['\r', '\n'], " "))
    }
}

/// Lowercase words admitted between the capitalised words of a party.
/// `ex rel.` is the only supported multiword connector.
fn party_connector(word: &str) -> bool {
    matches!(
        word,
        "of" | "the" | "and" | "de" | "du" | "van" | "von" | "der" | "la" | "le" | "ex" | "rel"
    )
}

/// Whether a citation at `at` opens its line in `text`: only whitespace
/// and Markdown marks before it, and no citation signal first ("See Grant
/// v. Finecy, …": a sentence's first word joins the party).
fn opens_line(text: &str, at: usize) -> bool {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let first = text[at..]
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or_default();
    text[start..at]
        .chars()
        .all(|c| c.is_whitespace() || "#>*_|".contains(c))
        && !matches!(
            first,
            "See" | "see" | "Cf" | "cf" | "Accord" | "Compare" | "Contra" | "But" | "Also"
        )
}

/// A citation may be the document's own caption. Require its own line
/// (allowing Markdown and the court after a citation), or a judgment
/// label in the next three nonempty lines. Prose before/after a citation
/// alone, such as "Motion under Pellam ...", does not establish a caption.
fn cited_caption(text: &str, case: &Cited) -> bool {
    let start = text[..case.range.start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[case.range.end..]
        .find('\n')
        .map_or(text.len(), |i| case.range.end + i);
    let unmarked = |s: &str| {
        s.trim_matches(|c: char| c.is_whitespace() || "#*_".contains(c))
            .to_owned()
    };
    static COURT: OnceLock<Regex> = OnceLock::new();
    let standalone = unmarked(&text[start..case.range.start]).is_empty()
        && COURT
            .get_or_init(|| Regex::new(r"^(?:\s*\([^()\n]*\))*[.,;:]?$").expect("a pattern"))
            .is_match(&unmarked(&text[case.range.end..end]));
    standalone
        || text[end..]
            .lines()
            .map(unmarked)
            .filter(|line| !line.is_empty())
            .take(3)
            .any(|line| {
                matches!(
                    line.as_str(),
                    "Judgment"
                        | "JUDGMENT"
                        | "Approved Judgment"
                        | "Opinion"
                        | "OPINION"
                        | "Order"
                        | "ORDER"
                ) || line.starts_with("Before:")
            })
}

/// Whether the break between lines `first` and `second` may wrap running
/// text: neither is blank, a heading or a table row, and the second opens
/// no quote, list item or numbered item.
pub(super) fn soft_break(first: &str, second: &str) -> bool {
    let (first, second) = (first.trim(), second.trim());
    let numbered = second
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .starts_with(['.', ')'])
        && second.starts_with(|c: char| c.is_ascii_digit());
    !first.is_empty()
        && !second.is_empty()
        && !first.starts_with(['|', '#'])
        && !second.starts_with(['|', '#', '>', '-', '*', '+'])
        && !numbered
}
