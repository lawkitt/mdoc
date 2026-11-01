//! Recognition fixtures and their scoring (ADR 0093): one Markdown file per
//! fixture with front matter and `⟦CATEGORY|text⟧` gold, its raw model
//! snapshot `<id>.model.json` beside it, and what the user would be shown
//! for it: the correction stage, then the review's own overlap rules and
//! repeats. Used by `just recognize`, the snapshot test and the detection
//! gate; the app never reads fixtures.
use crate::{RawEntity, correct};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    ops::Range,
    path::{Path, PathBuf},
};

/// Gold categories, as the annotation guide names them.
pub const CATEGORIES: [&str; 10] = [
    "PERSON", "ORG", "ADDRESS", "DATE", "CASE", "EMAIL", "PHONE", "IDENTITY", "TAX", "BANK",
];
/// Document kinds of the guide's coverage matrix.
pub const KINDS: [&str; 8] = [
    "contract",
    "claim",
    "judgment",
    "power-of-attorney",
    "letter",
    "ocr",
    "docx",
    "tables",
];
const LANGUAGES: [&str; 2] = ["en", "ru"];

/// A fixture's expected result: `pass` must pass, `known-flaw` must fail
/// (strict: a fix that makes it pass fails until it flips) and
/// `model-limit` is listed, never asserted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Pass,
    KnownFlaw,
    ModelLimit,
}
impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::KnownFlaw => "known-flaw",
            Self::ModelLimit => "model-limit",
        }
    }
}

/// A span of the source: gold or proposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub category: String,
}
impl Ord for Span {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.range.start, self.range.end, &self.category).cmp(&(
            other.range.start,
            other.range.end,
            &other.category,
        ))
    }
}
impl PartialOrd for Span {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// One fixture file, parsed.
#[derive(Clone, Debug)]
pub struct Fixture {
    pub path: PathBuf,
    pub id: String,
    /// `en` or `ru`: the folder above the kind's, or front matter
    /// `language` outside the repository's layout; `?` when neither says.
    pub language: String,
    /// None for a file without front matter (a plain document to annotate).
    pub status: Option<Status>,
    pub flaw: String,
    pub issue: String,
    pub kind: String,
    pub origin: String,
    pub reason: String,
    /// The document with its gold marks removed.
    pub source: String,
    pub gold: Vec<Span>,
}

impl Fixture {
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(path, &text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(path: &Path, text: &str) -> Result<Self, String> {
        let mut fixture = Fixture {
            path: path.to_owned(),
            id: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_owned(),
            language: String::new(),
            status: None,
            flaw: String::new(),
            issue: String::new(),
            kind: String::new(),
            origin: String::new(),
            reason: String::new(),
            source: String::new(),
            gold: Vec::new(),
        };
        let mut body = text;
        let mut language = String::new();
        if let Some(rest) = text.strip_prefix("---\n") {
            let end = rest
                .find("\n---\n")
                .ok_or("front matter has no closing `---` line")?;
            for line in rest[..end].lines() {
                if line.trim().is_empty() || line.trim_start().starts_with('#') {
                    continue;
                }
                let (key, value) = line
                    .split_once(':')
                    .ok_or_else(|| format!("front matter line without `key:`: {line}"))?;
                let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
                match key.trim() {
                    "status" => {
                        fixture.status = Some(match value.as_str() {
                            "pass" => Status::Pass,
                            "known-flaw" => Status::KnownFlaw,
                            "model-limit" => Status::ModelLimit,
                            other => return Err(format!("unknown status `{other}`")),
                        })
                    }
                    "flaw" => fixture.flaw = value,
                    "issue" => fixture.issue = value,
                    "kind" => fixture.kind = value,
                    "source" => fixture.origin = value,
                    "reason" => fixture.reason = value,
                    "language" => language = value,
                    other => return Err(format!("unknown front matter key `{other}`")),
                }
            }
            body = &rest[end + "\n---\n".len()..];
        }
        let (source, gold) = strip_gold(body)?;
        fixture.source = source;
        fixture.gold = gold;
        let folders: Vec<_> = path
            .parent()
            .into_iter()
            .flat_map(|p| p.components().rev().take(2))
            .filter_map(|c| c.as_os_str().to_str())
            .collect();
        fixture.language = match folders.as_slice() {
            [kind, lang] if *kind == fixture.kind && LANGUAGES.contains(lang) => (*lang).to_owned(),
            _ if !language.is_empty() => language,
            [lang, ..] if LANGUAGES.contains(lang) => (*lang).to_owned(),
            _ => "?".to_owned(),
        };
        Ok(fixture)
    }

    /// The front matter rules of the guide; a fixture breaking one is an
    /// error whatever its result.
    pub fn validate(&self) -> Result<(), String> {
        let status = self.status.ok_or("no front matter")?;
        if !KINDS.contains(&self.kind.as_str()) {
            return Err(format!(
                "kind `{}` is not one of {}",
                self.kind,
                KINDS.join(", ")
            ));
        }
        let hunt = self
            .origin
            .strip_prefix("hunt-r")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if !(hunt || ["import", "demo"].contains(&self.origin.as_str())) {
            return Err(format!(
                "source `{}` is not import, demo or hunt-rN",
                self.origin
            ));
        }
        if status == Status::KnownFlaw && (self.flaw.is_empty() || self.issue.is_empty()) {
            return Err("a known-flaw fixture names its flaw class and issue".into());
        }
        if status == Status::ModelLimit && self.reason.is_empty() {
            return Err("a model-limit fixture gives its reason".into());
        }
        Ok(())
    }

    /// The repository's layout, `{en,ru}/<kind>/<id>.md`.
    pub fn validate_layout(&self) -> Result<(), String> {
        let folders: Vec<_> = self
            .path
            .parent()
            .into_iter()
            .flat_map(|p| p.components().rev().take(2))
            .filter_map(|c| c.as_os_str().to_str())
            .collect();
        match folders.as_slice() {
            [kind, lang] if *kind == self.kind && LANGUAGES.contains(lang) => Ok(()),
            _ => Err(format!(
                "not under {{en,ru}}/{}/ (the folder names the language and kind)",
                self.kind
            )),
        }
    }

    pub fn snapshot_path(&self) -> PathBuf {
        self.path.with_file_name(format!("{}.model.json", self.id))
    }
}

/// `⟦CATEGORY|text⟧` marks out, with their spans in the result.
pub fn strip_gold(annotated: &str) -> Result<(String, Vec<Span>), String> {
    let mut source = String::with_capacity(annotated.len());
    let mut gold = Vec::new();
    let mut tail = annotated;
    while let Some(open) = tail.find(['⟦', '⟧']) {
        if tail[open..].starts_with('⟧') {
            return Err(format!("`⟧` without `⟦` after {:?}", tail_context(&source)));
        }
        source.push_str(&tail[..open]);
        let mark = &tail[open + '⟦'.len_utf8()..];
        let close = mark
            .find('⟧')
            .ok_or_else(|| format!("`⟦` without `⟧` after {:?}", tail_context(&source)))?;
        let inner = &mark[..close];
        if inner.contains('⟦') {
            return Err(format!("nested `⟦` after {:?}", tail_context(&source)));
        }
        let (category, text) = inner
            .split_once('|')
            .ok_or_else(|| format!("`⟦{inner}⟧` has no `|`"))?;
        if !CATEGORIES.contains(&category) {
            return Err(format!(
                "unknown category `{category}` (one of {})",
                CATEGORIES.join(", ")
            ));
        }
        if text.is_empty() || text.trim() != text {
            return Err(format!("`⟦{inner}⟧`: empty or space at an edge"));
        }
        let start = source.len();
        source.push_str(text);
        gold.push(Span {
            range: start..source.len(),
            category: category.to_owned(),
        });
        tail = &mark[close + '⟧'.len_utf8()..];
    }
    source.push_str(tail);
    Ok((source, gold))
}

fn tail_context(source: &str) -> &str {
    let mut start = source.len().saturating_sub(30);
    while !source.is_char_boundary(start) {
        start += 1;
    }
    &source[start..]
}

/// A source with spans marked the way gold is.
pub fn annotate(source: &str, spans: &[Span]) -> String {
    let mut spans: Vec<_> = spans.iter().collect();
    spans.sort();
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for span in spans {
        if span.range.start < at {
            continue;
        }
        out.push_str(&source[at..span.range.start]);
        let _ = write!(out, "⟦{}|{}⟧", span.category, &source[span.range.clone()]);
        at = span.range.end;
    }
    out.push_str(&source[at..]);
    out
}

/// A fixture's committed raw model output (ADR 0093), made on the Mac.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// The model it came from: [`crate::model_name`].
    pub model: String,
    pub threshold: f32,
    pub entities: Vec<RawEntity>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SnapshotHeader {
    model: String,
    threshold: f32,
    entities: Vec<RawEntity>,
}

impl Snapshot {
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let header: SnapshotHeader =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self {
            model: header.model,
            threshold: header.threshold,
            entities: header.entities,
        })
    }

    /// One entity per line, so a refresh diffs by span.
    pub fn to_json(&self) -> String {
        let mut out = format!(
            "{{\n  \"model\": {},\n  \"threshold\": {},\n  \"entities\": [",
            serde_json::to_string(&self.model).expect("string"),
            serde_json::to_string(&self.threshold).expect("number"),
        );
        for (index, entity) in self.entities.iter().enumerate() {
            out.push_str(if index == 0 { "\n    " } else { ",\n    " });
            out.push_str(&serde_json::to_string(entity).expect("entity"));
        }
        out.push_str(if self.entities.is_empty() {
            "]\n}\n"
        } else {
            "\n  ]\n}\n"
        });
        out
    }

    /// Every entity's offsets name its text in `source`.
    pub fn validate(&self, source: &str) -> Result<(), String> {
        for entity in &self.entities {
            if entity.start >= entity.end
                || source.get(entity.start..entity.end) != Some(entity.text.as_str())
            {
                return Err(format!(
                    "snapshot entity {}..{} {:?} does not match the source; refresh it",
                    entity.start, entity.end, entity.text
                ));
            }
        }
        Ok(())
    }

    /// Why `fresh` differs from this snapshot, if it does: the model, the
    /// threshold, or the first entity that changed. Scores may differ in
    /// the last bits between runs.
    pub fn stale(&self, fresh: &Snapshot) -> Option<String> {
        if self.model != fresh.model {
            return Some(format!("made with {}, not {}", self.model, fresh.model));
        }
        if self.threshold != fresh.threshold {
            return Some(format!(
                "made at threshold {}, not {}",
                self.threshold, fresh.threshold
            ));
        }
        let same = |a: &RawEntity, b: &RawEntity| {
            a.start == b.start
                && a.end == b.end
                && a.label == b.label
                && a.text == b.text
                && (a.score - b.score).abs() <= 1e-4
        };
        for index in 0..self.entities.len().max(fresh.entities.len()) {
            match (self.entities.get(index), fresh.entities.get(index)) {
                (Some(a), Some(b)) if same(a, b) => {}
                (a, b) => {
                    let show = |e: Option<&RawEntity>| {
                        e.map_or("nothing".to_owned(), |e| {
                            format!(
                                "{} {:?} {}..{} ({:.3})",
                                e.label, e.text, e.start, e.end, e.score
                            )
                        })
                    };
                    return Some(format!(
                        "entity {}: snapshot has {}, the model gives {}",
                        index + 1,
                        show(a),
                        show(b)
                    ));
                }
            }
        }
        None
    }
}

/// What the user would be shown for these raw entities: [`correct`], then
/// the review's conflict rules and each wording's other mentions.
pub fn proposals(source: &str, raw: &[RawEntity]) -> Result<Vec<Span>, String> {
    let mut review = mdoc_pii::Review::default();
    review.ingest(source, correct(source, raw))?;
    let mut spans: Vec<Span> = review
        .candidates()
        .iter()
        .filter_map(|candidate| {
            let variant = review.variant(candidate.variant)?;
            Some(Span {
                range: candidate.range.clone(),
                category: variant.category.token().to_owned(),
            })
        })
        .collect();
    spans.sort();
    spans.dedup();
    Ok(spans)
}

/// Proposals against gold: exact hits, and the rest as misses (gold nothing
/// proposed touches), false hits (proposals touching no gold) and boundary
/// errors (overlapping gold and proposals that differ in range or category).
#[derive(Clone, Debug, Default)]
pub struct Comparison {
    pub hits: Vec<Span>,
    pub misses: Vec<Span>,
    pub false_hits: Vec<Span>,
    pub boundary: Vec<(Vec<Span>, Vec<Span>)>,
}

impl Comparison {
    pub fn new(gold: &[Span], proposed: &[Span]) -> Self {
        let mut comparison = Self::default();
        let mut gold_left = Vec::new();
        for span in gold {
            if proposed.contains(span) {
                comparison.hits.push(span.clone());
            } else {
                gold_left.push(span.clone());
            }
        }
        let proposed_left: Vec<Span> = proposed
            .iter()
            .filter(|span| !gold.contains(span))
            .cloned()
            .collect();
        let overlaps = |a: &Range<usize>, b: &Range<usize>| a.start < b.end && b.start < a.end;
        // Connected groups of overlapping leftovers.
        let mut gold_group = vec![usize::MAX; gold_left.len()];
        let mut proposed_group = vec![usize::MAX; proposed_left.len()];
        let mut groups = 0;
        for start in 0..gold_left.len() {
            if gold_group[start] != usize::MAX {
                continue;
            }
            let group = groups;
            groups += 1;
            gold_group[start] = group;
            let mut queue = vec![(true, start)];
            while let Some((is_gold, index)) = queue.pop() {
                let range = if is_gold {
                    gold_left[index].range.clone()
                } else {
                    proposed_left[index].range.clone()
                };
                if is_gold {
                    for (other, span) in proposed_left.iter().enumerate() {
                        if proposed_group[other] == usize::MAX && overlaps(&range, &span.range) {
                            proposed_group[other] = group;
                            queue.push((false, other));
                        }
                    }
                } else {
                    for (other, span) in gold_left.iter().enumerate() {
                        if gold_group[other] == usize::MAX && overlaps(&range, &span.range) {
                            gold_group[other] = group;
                            queue.push((true, other));
                        }
                    }
                }
            }
        }
        for group in 0..groups {
            let golds: Vec<Span> = gold_left
                .iter()
                .zip(&gold_group)
                .filter(|(_, g)| **g == group)
                .map(|(s, _)| s.clone())
                .collect();
            let props: Vec<Span> = proposed_left
                .iter()
                .zip(&proposed_group)
                .filter(|(_, g)| **g == group)
                .map(|(s, _)| s.clone())
                .collect();
            if props.is_empty() {
                comparison.misses.extend(golds);
            } else {
                comparison.boundary.push((golds, props));
            }
        }
        comparison.false_hits = proposed_left
            .into_iter()
            .zip(proposed_group)
            .filter(|(_, g)| *g == usize::MAX)
            .map(|(s, _)| s)
            .collect();
        comparison
    }

    pub fn passes(&self) -> bool {
        self.misses.is_empty() && self.false_hits.is_empty() && self.boundary.is_empty()
    }

    /// One line per kind of flaw, quoting the source; a flaw repeated word
    /// for word is counted (`×40`).
    pub fn report(&self, source: &str) -> String {
        let show = |span: &Span| format!("{} {:?}", span.category, &source[span.range.clone()]);
        let list = |spans: &[Span]| spans.iter().map(show).collect::<Vec<_>>().join(", ");
        let mut lines: Vec<(String, usize)> = Vec::new();
        let flaws = self
            .misses
            .iter()
            .map(|span| format!("miss: {}", show(span)))
            .chain(
                self.false_hits
                    .iter()
                    .map(|span| format!("false hit: {}", show(span))),
            )
            .chain(self.boundary.iter().map(|(gold, proposed)| {
                format!("boundary: gold {}; proposed {}", list(gold), list(proposed))
            }));
        for line in flaws {
            match lines.iter_mut().find(|(seen, _)| *seen == line) {
                Some((_, count)) => *count += 1,
                None => lines.push((line, 1)),
            }
        }
        let mut out = String::new();
        for (line, count) in lines {
            if count == 1 {
                let _ = writeln!(out, "  {line}");
            } else {
                let _ = writeln!(out, "  {line} ×{count}");
            }
        }
        out
    }
}

/// A fixture's result against its status: `None` when it is as expected.
pub fn ratchet(status: Status, passes: bool) -> Option<&'static str> {
    match (status, passes) {
        (Status::Pass, false) => Some("fails, but its status is pass"),
        (Status::KnownFlaw, true) => Some("passes now: flip its status to pass"),
        _ => None,
    }
}

/// Every `*.md` fixture under `root`, sorted, skipping `README.md` and
/// `GUIDE.md`; `root` may also be one file.
pub fn fixture_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    if root.is_file() {
        return Ok(vec![root.to_owned()]);
    }
    let mut paths = Vec::new();
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "md")
                && !matches!(
                    path.file_name().and_then(|n| n.to_str()),
                    Some("README.md" | "GUIDE.md")
                )
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

/// Recall and precision per language × category × kind, and fixtures per
/// status (ADR 0093). Counts are wordings per fixture: a repeated mention
/// with the same result counts once.
#[derive(Clone, Debug, Default)]
pub struct Scoreboard {
    /// (language, category, kind) → [gold, gold found, proposed, proposed right].
    cells: BTreeMap<(String, String, String), [usize; 4]>,
    statuses: BTreeMap<&'static str, usize>,
    fixtures: usize,
    passing: usize,
}

impl Scoreboard {
    pub fn add(&mut self, fixture: &Fixture, comparison: &Comparison, proposed: &[Span]) {
        self.fixtures += 1;
        if comparison.passes() {
            self.passing += 1;
        }
        *self
            .statuses
            .entry(fixture.status.map_or("none", Status::name))
            .or_default() += 1;
        let cell = |category: &str| {
            (
                fixture.language.clone(),
                category.to_owned(),
                if fixture.kind.is_empty() {
                    "?".to_owned()
                } else {
                    fixture.kind.clone()
                },
            )
        };
        // A wording repeated word for word with the same result counts once
        // per fixture, so a long document of repeats weighs as one section.
        let text = |span: &Span| &fixture.source[span.range.clone()];
        let gold: std::collections::BTreeSet<_> = fixture
            .gold
            .iter()
            .map(|span| (&span.category, text(span), comparison.hits.contains(span)))
            .collect();
        for (category, _, hit) in gold {
            let counts = self.cells.entry(cell(category)).or_default();
            counts[0] += 1;
            counts[1] += usize::from(hit);
        }
        let proposed: std::collections::BTreeSet<_> = proposed
            .iter()
            .map(|span| (&span.category, text(span), comparison.hits.contains(span)))
            .collect();
        for (category, _, right) in proposed {
            let counts = self.cells.entry(cell(category)).or_default();
            counts[2] += 1;
            counts[3] += usize::from(right);
        }
    }

    pub fn render(&self) -> String {
        let percent = |part: usize, whole: usize| {
            if whole == 0 {
                "-".to_owned()
            } else {
                format!("{:.1}%", 100.0 * part as f64 / whole as f64)
            }
        };
        let mut out =
            String::from("Wordings per fixture (a repeat with the same result counts once).\n");
        let _ = writeln!(
            out,
            "{:<4} {:<9} {:<18} {:>5} {:>5} {:>7} {:>8} {:>5} {:>9}",
            "lang", "category", "kind", "gold", "found", "recall", "proposed", "right", "precision"
        );
        let mut totals: BTreeMap<String, [usize; 4]> = BTreeMap::new();
        for ((language, category, kind), c) in &self.cells {
            let _ = writeln!(
                out,
                "{:<4} {:<9} {:<18} {:>5} {:>5} {:>7} {:>8} {:>5} {:>9}",
                language,
                category,
                kind,
                c[0],
                c[1],
                percent(c[1], c[0]),
                c[2],
                c[3],
                percent(c[3], c[2])
            );
            for key in [language.clone(), "all".to_owned()] {
                let total = totals.entry(key).or_default();
                for (sum, value) in total.iter_mut().zip(c) {
                    *sum += value;
                }
            }
        }
        for (language, c) in &totals {
            let _ = writeln!(
                out,
                "{:<4} {:<9} {:<18} {:>5} {:>5} {:>7} {:>8} {:>5} {:>9}",
                language,
                "all",
                "all",
                c[0],
                c[1],
                percent(c[1], c[0]),
                c[2],
                c[3],
                percent(c[3], c[2])
            );
        }
        let statuses: Vec<String> = self
            .statuses
            .iter()
            .map(|(status, count)| format!("{status} {count}"))
            .collect();
        let _ = writeln!(
            out,
            "\n{} fixtures, {} passing; by status: {}",
            self.fixtures,
            self.passing,
            statuses.join(", ")
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(range: Range<usize>, category: &str) -> Span {
        Span {
            range,
            category: category.into(),
        }
    }

    #[test]
    fn gold_marks_come_out_with_their_spans() {
        let (source, gold) = strip_gold("Ms ⟦PERSON|Bennett⟧ of ⟦ORG|ООО «Север»⟧.").unwrap();
        assert_eq!(source, "Ms Bennett of ООО «Север».");
        assert_eq!(&source[gold[0].range.clone()], "Bennett");
        assert_eq!(&source[gold[1].range.clone()], "ООО «Север»");
        assert_eq!(gold[1].category, "ORG");
        for bad in [
            "⟦PERSON|Ann",
            "Ann⟧",
            "⟦NAME|Ann⟧",
            "⟦PERSON Ann⟧",
            "⟦PERSON| Ann⟧",
            "⟦PERSON|⟦ORG|A⟧⟧",
        ] {
            assert!(strip_gold(bad).is_err(), "{bad}");
        }
        assert_eq!(
            annotate(&source, &gold),
            "Ms ⟦PERSON|Bennett⟧ of ⟦ORG|ООО «Север»⟧."
        );
    }

    #[test]
    fn front_matter_names_status_kind_and_language() {
        let path = Path::new("tests/fixtures/recognition/ru/claim/x.md");
        let fixture = Fixture::parse(
            path,
            "---\nstatus: known-flaw\nflaw: ru-address\nissue: 362\nkind: claim\nsource: hunt-r1\n---\nАдрес: ⟦ADDRESS|ул. Ленина, д. 1⟧\n",
        )
        .unwrap();
        assert_eq!(fixture.status, Some(Status::KnownFlaw));
        assert_eq!(fixture.language, "ru");
        assert_eq!(fixture.source, "Адрес: ул. Ленина, д. 1\n");
        fixture.validate().unwrap();
        fixture.validate_layout().unwrap();
        let wrong = |front: &str| {
            Fixture::parse(path, &format!("---\n{front}\n---\nx")).and_then(|f| f.validate())
        };
        assert!(wrong("status: known-flaw\nkind: claim\nsource: import").is_err());
        assert!(wrong("status: model-limit\nkind: claim\nsource: import").is_err());
        assert!(wrong("status: pass\nkind: memo\nsource: import").is_err());
        assert!(wrong("status: pass\nkind: claim\nsource: hunt").is_err());
        assert!(wrong("status: done\nkind: claim\nsource: import").is_err());
        assert!(wrong("status: pass\nkind: claim\nsource: import\nflavor: x").is_err());
        wrong("status: pass\nkind: claim\nsource: import").unwrap();
        let outside = Fixture::parse(
            Path::new("/holdout/doc-1.md"),
            "---\nstatus: pass\nkind: letter\nsource: import\nlanguage: en\n---\nx",
        )
        .unwrap();
        assert_eq!(outside.language, "en");
        assert!(outside.validate_layout().is_err());
    }

    #[test]
    fn comparison_sorts_flaws_into_misses_false_hits_and_boundaries() {
        let gold = [
            span(0..5, "PERSON"),
            span(10..20, "ADDRESS"),
            span(30..35, "DATE"),
            span(40..45, "ORG"),
        ];
        let proposed = [
            span(0..5, "PERSON"),
            span(10..15, "ADDRESS"),
            span(40..45, "PERSON"),
            span(50..55, "EMAIL"),
        ];
        let comparison = Comparison::new(&gold, &proposed);
        assert_eq!(comparison.hits, [span(0..5, "PERSON")]);
        assert_eq!(comparison.misses, [span(30..35, "DATE")]);
        assert_eq!(comparison.false_hits, [span(50..55, "EMAIL")]);
        assert_eq!(comparison.boundary.len(), 2);
        assert!(!comparison.passes());
        assert!(Comparison::new(&gold, &gold).passes());
        assert_eq!(
            ratchet(Status::KnownFlaw, true),
            Some("passes now: flip its status to pass")
        );
        assert_eq!(
            ratchet(Status::Pass, false),
            Some("fails, but its status is pass")
        );
        assert_eq!(ratchet(Status::ModelLimit, true), None);
    }

    #[test]
    fn proposals_follow_the_review_rules() {
        let source = "Ann Lee met Ann Lee at ann@example.invalid.";
        let raw = [
            RawEntity {
                start: 0,
                end: 7,
                label: "person".into(),
                score: 0.9,
                text: "Ann Lee".into(),
            },
            RawEntity {
                start: 23,
                end: 42,
                label: "email".into(),
                score: 0.8,
                text: "ann@example.invalid".into(),
            },
        ];
        // The other "Ann Lee" is found as a repeat; the email rule and the
        // model's email are one proposal.
        assert_eq!(
            proposals(source, &raw).unwrap(),
            [
                span(0..7, "PERSON"),
                span(12..19, "PERSON"),
                span(23..42, "EMAIL")
            ]
        );
    }

    #[test]
    fn the_scoreboard_counts_a_repeated_wording_once() {
        let fixture = Fixture::parse(
            Path::new("en/letter/x.md"),
            "---\nstatus: known-flaw\nflaw: f\nissue: 1\nkind: letter\nsource: import\n---\n⟦PERSON|Ann⟧ and ⟦PERSON|Ann⟧ met ⟦PERSON|Bob⟧.",
        )
        .unwrap();
        let proposed = [
            span(0..3, "PERSON"),
            span(8..11, "PERSON"),
            span(16..19, "ORG"),
        ];
        let comparison = Comparison::new(&fixture.gold, &proposed);
        let mut board = Scoreboard::default();
        board.add(&fixture, &comparison, &proposed);
        let cells: Vec<_> = board
            .cells
            .iter()
            .map(|(k, v)| (k.1.as_str(), *v))
            .collect();
        assert_eq!(cells, [("ORG", [0, 0, 1, 0]), ("PERSON", [2, 1, 1, 1])]);
        assert!(board.render().contains("known-flaw 1"));
    }

    #[test]
    fn snapshots_round_trip_and_name_what_went_stale() {
        let snapshot = Snapshot {
            model: "m".into(),
            threshold: 0.5,
            entities: vec![RawEntity {
                start: 0,
                end: 3,
                label: "person".into(),
                score: 0.876_543_2,
                text: "Ann".into(),
            }],
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.model.json");
        fs::write(&path, snapshot.to_json()).unwrap();
        let read = Snapshot::read(&path).unwrap();
        assert_eq!(read, snapshot);
        assert_eq!(read.stale(&snapshot), None);
        read.validate("Ann").unwrap();
        assert!(read.validate("Bob").is_err());
        let mut moved = snapshot.clone();
        moved.entities[0].end = 2;
        assert!(snapshot.stale(&moved).unwrap().contains("entity 1"));
        moved.entities.clear();
        assert!(snapshot.stale(&moved).unwrap().contains("nothing"));
        let empty = Snapshot {
            entities: Vec::new(),
            ..snapshot.clone()
        };
        fs::write(&path, empty.to_json()).unwrap();
        assert_eq!(Snapshot::read(&path).unwrap(), empty);
    }
}
