//! Live-document review policy. Source text is immutable during a detection job;
//! every edit plan is checked against the current source and editor revision.
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Person,
    Organization,
    Email,
    Phone,
    Address,
    Identity,
    Tax,
    Bank,
}
impl Category {
    pub const ALL: [Self; 8] = [
        Self::Person,
        Self::Organization,
        Self::Email,
        Self::Phone,
        Self::Address,
        Self::Identity,
        Self::Tax,
        Self::Bank,
    ];
    pub fn token(self) -> &'static str {
        match self {
            Self::Person => "PERSON",
            Self::Organization => "ORG",
            Self::Email => "EMAIL",
            Self::Phone => "PHONE",
            Self::Address => "ADDRESS",
            Self::Identity => "IDENTITY",
            Self::Tax => "TAX",
            Self::Bank => "BANK",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Detection {
    pub range: Range<usize>,
    pub category: Category,
    pub score: f32,
}
#[derive(Clone, Debug)]
pub struct Group {
    pub id: u64,
    pub original: String,
    pub category: Category,
    pub replacement: String,
    pub mentions: Vec<Range<usize>>,
    kept: bool,
}
#[derive(Clone, Debug)]
struct Exclusion {
    range: Range<usize>,
    original: String,
}

#[derive(Default)]
pub struct Review {
    pub open: bool,
    pub skipped_syntax_spans: usize,
    source: String,
    pub groups: Vec<Group>,
    exclusions: Vec<Exclusion>,
    counters: HashMap<Category, usize>,
    next_id: u64,
}

/// Tokens work unchanged in prose, table cells, destinations, code and HTML.
/// Custom replacements must not introduce Markdown/URL/HTML delimiters.
pub fn valid_replacement(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}
fn safe_span(value: &str) -> bool {
    value.len() <= 1024
        && value.chars().any(char::is_alphanumeric)
        && !value.starts_with('_')
        && !value.ends_with('_')
        && !value.chars().any(|ch| "\n\r[]<>`*|\\#~".contains(ch))
}
fn occurrences(source: &str, original: &str) -> Vec<Range<usize>> {
    source
        .match_indices(original)
        .filter_map(|(start, value)| {
            let end = start + value.len();
            // Avoid turning Ann into PERSON_1 inside Anna. Underscore-separated
            // hidden paths remain exact-repeat candidates, as agreed for full source.
            let left = original.chars().next().is_some_and(char::is_alphanumeric)
                && source[..start]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric);
            let right = original
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
                && source[end..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric);
            (!left && !right).then_some(start..end)
        })
        .collect()
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// Protect letter/number-bearing grammar: list prefixes, HTML tag and attribute
/// names, and attribute quotes. Identifying-information detection stays in GLiNER2.
fn protected_syntax(source: &str) -> Vec<Range<usize>> {
    let mut protected = Vec::new();
    let mut base = 0;
    for line in source.split_inclusive('\n') {
        let mut start = line.len() - line.trim_start_matches([' ', '\t']).len();
        while line[start..].starts_with('>') {
            start += 1;
            start += line[start..].len() - line[start..].trim_start_matches([' ', '\t']).len();
        }
        let body = &line[start..];
        if body.starts_with("- ") || body.starts_with("+ ") || body.starts_with("* ") {
            protected.push(base + start..base + start + 2);
        }
        let digits = body.bytes().take_while(u8::is_ascii_digit).count();
        if (1..=9).contains(&digits)
            && matches!(body.as_bytes().get(digits), Some(b'.' | b')'))
            && body
                .as_bytes()
                .get(digits + 1)
                .is_some_and(u8::is_ascii_whitespace)
        {
            protected.push(base + start..base + start + digits + 1);
        }
        base += line.len();
    }
    let bytes = source.as_bytes();
    let name_byte = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':');
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'<' {
            at += 1;
            continue;
        }
        let mut i = at + 1;
        if bytes.get(i) == Some(&b'/') {
            i += 1;
        }
        if !bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
            at += 1;
            continue;
        }
        let tag = i;
        while bytes.get(i).is_some_and(|byte| name_byte(*byte)) {
            i += 1;
        }
        protected.push(tag..i);
        loop {
            while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b'>' {
                break;
            }
            if bytes[i] == b'/' {
                i += 1;
                continue;
            }
            let name = i;
            while bytes.get(i).is_some_and(|byte| name_byte(*byte)) {
                i += 1;
            }
            if i == name {
                break;
            }
            protected.push(name..i);
            while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            if bytes.get(i) != Some(&b'=') {
                continue;
            }
            i += 1;
            while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                i += 1;
            }
            if let Some(quote @ (b'\'' | b'"')) = bytes.get(i).copied() {
                protected.push(i..i + 1);
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                if i < bytes.len() {
                    protected.push(i..i + 1);
                    i += 1;
                }
            } else {
                while bytes
                    .get(i)
                    .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'>')
                {
                    i += 1;
                }
            }
        }
        at = i.saturating_add(1);
    }
    // Link outer delimiters remain intact, while ordinary parenthesized
    // phone/address text and balanced parentheses inside destinations are valid.
    for (open, _) in source.match_indices("](") {
        let mut depth = 1;
        let mut i = open + 2;
        let mut quote = None;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if let Some(current) = quote {
                if bytes[i] == current {
                    protected.push(i..i + 1);
                    quote = None;
                }
            } else if matches!(bytes[i], b'\'' | b'"')
                && bytes.get(i - 1).is_some_and(u8::is_ascii_whitespace)
            {
                quote = Some(bytes[i]);
                protected.push(i..i + 1);
            } else if bytes[i] == b'(' {
                depth += 1;
            } else if bytes[i] == b')' {
                depth -= 1;
                if depth == 0 {
                    protected.push(i..i + 1);
                    break;
                }
            }
            i += 1;
        }
    }
    protected
}

impl Review {
    pub fn group(&self, id: u64) -> Option<&Group> {
        self.groups.iter().find(|group| group.id == id)
    }
    pub fn remaining(&self) -> usize {
        self.groups.iter().map(|group| group.mentions.len()).sum()
    }
    pub fn mappings(&self) -> Vec<(String, String)> {
        let mut seen = HashSet::new();
        self.groups
            .iter()
            .filter(|group| seen.insert(group.replacement.clone()))
            .map(|group| (group.original.clone(), group.replacement.clone()))
            .collect()
    }
    pub fn refresh(&mut self, source: &str) {
        // A conservative source diff revalidates single-occurrence exclusions.
        // Anything crossing an edited region is invalidated rather than shifted
        // speculatively. Group seeds/mappings survive edits and undo.
        if self.source != source {
            let mut prefix = self
                .source
                .bytes()
                .zip(source.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            while !self.source.is_char_boundary(prefix) || !source.is_char_boundary(prefix) {
                prefix -= 1;
            }
            let mut suffix = self.source[prefix..]
                .bytes()
                .rev()
                .zip(source[prefix..].bytes().rev())
                .take_while(|(a, b)| a == b)
                .count();
            while !self.source.is_char_boundary(self.source.len() - suffix)
                || !source.is_char_boundary(source.len() - suffix)
            {
                suffix -= 1;
            }
            let old_end = self.source.len() - suffix;
            let delta = source.len() as isize - self.source.len() as isize;
            self.exclusions.retain_mut(|excluded| {
                if excluded.range.end <= prefix {
                    return source.get(excluded.range.clone()) == Some(excluded.original.as_str());
                }
                if excluded.range.start < old_end {
                    return false;
                }
                excluded.range = excluded.range.start.saturating_add_signed(delta)
                    ..excluded.range.end.saturating_add_signed(delta);
                source.get(excluded.range.clone()) == Some(excluded.original.as_str())
            });
            self.source = source.to_owned();
        }
        let protected = protected_syntax(source);
        let mut occupied: Vec<Range<usize>> = Vec::new();
        for group in &mut self.groups {
            group.mentions = if group.kept {
                Vec::new()
            } else {
                occurrences(source, &group.original)
                    .into_iter()
                    .filter(|range| {
                        !protected.iter().any(|syntax| overlaps(range, syntax))
                            && !self.exclusions.iter().any(|excluded| {
                                excluded.original == group.original && excluded.range == *range
                            })
                            && !occupied
                                .iter()
                                .any(|other| range.start < other.end && other.start < range.end)
                    })
                    .collect()
            };
            occupied.extend(group.mentions.iter().cloned());
        }
    }
    /// Rebase Keep decisions using the exact batch just committed by the editor.
    /// A broad source diff would discard unchanged exclusions between edits.
    pub fn refresh_after_edits(&mut self, source: &str, edits: &[(Range<usize>, String)]) {
        self.exclusions.retain_mut(|excluded| {
            if edits
                .iter()
                .any(|(range, _)| overlaps(range, &excluded.range))
            {
                return false;
            }
            let delta: isize = edits
                .iter()
                .filter(|(range, _)| range.end <= excluded.range.start)
                .map(|(range, replacement)| replacement.len() as isize - range.len() as isize)
                .sum();
            excluded.range = excluded.range.start.saturating_add_signed(delta)
                ..excluded.range.end.saturating_add_signed(delta);
            source.get(excluded.range.clone()) == Some(excluded.original.as_str())
        });
        self.source = source.to_owned();
        self.refresh(source);
    }
    pub fn ingest(&mut self, source: &str, mut detections: Vec<Detection>) -> Result<(), String> {
        if detections.iter().any(|detected| {
            detected.range.start >= detected.range.end
                || source.get(detected.range.clone()).is_none()
                || !detected.score.is_finite()
        }) {
            return Err("Invalid detector offsets; scan discarded.".into());
        }
        // Prefer the strongest model span in overlapping/category conflicts.
        detections.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(b.range.len().cmp(&a.range.len()))
                .then(a.range.start.cmp(&b.range.start))
        });
        let protected = protected_syntax(source);
        self.skipped_syntax_spans = 0;
        let mut accepted: Vec<Range<usize>> = Vec::new();
        let mut selected = Vec::new();
        for detection in detections {
            if !safe_span(&source[detection.range.clone()])
                || protected
                    .iter()
                    .any(|syntax| overlaps(&detection.range, syntax))
            {
                self.skipped_syntax_spans += 1;
                continue;
            }
            if accepted
                .iter()
                .any(|other| overlaps(&detection.range, other))
            {
                continue;
            }
            accepted.push(detection.range.clone());
            selected.push(detection);
        }
        self.refresh(source);
        selected.sort_by_key(|detection| detection.range.start);
        for detection in selected {
            self.add_seed(&source[detection.range], detection.category);
        }
        self.refresh(source);
        Ok(())
    }
    fn add_seed(&mut self, original: &str, category: Category) -> u64 {
        if let Some(group) = self.groups.iter().find(|group| group.original == original) {
            return group.id;
        }
        let count = self.counters.entry(category).or_default();
        let replacement = loop {
            *count += 1;
            let value = format!("{}_{count}", category.token());
            if !self.groups.iter().any(|group| group.replacement == value)
                && !self.source.contains(&value)
            {
                break value;
            }
        };
        self.next_id += 1;
        let id = self.next_id;
        self.groups.push(Group {
            id,
            original: original.into(),
            category,
            replacement,
            mentions: Vec::new(),
            kept: false,
        });
        id
    }
    pub fn add_manual(
        &mut self,
        source: &str,
        range: Range<usize>,
        category: Category,
    ) -> Result<u64, String> {
        let protected = protected_syntax(source);
        let original = source.get(range.clone()).filter(|value| safe_span(value) && !protected.iter().any(|syntax| overlaps(&range, syntax))).ok_or("Select identifying text without Markdown delimiters. Narrow selections that cross syntax.")?;
        self.refresh(source);
        let id = self.add_seed(original, category);
        self.groups
            .iter_mut()
            .find(|group| group.id == id)
            .unwrap()
            .kept = false;
        self.exclusions
            .retain(|excluded| excluded.original != original);
        self.refresh(source);
        Ok(id)
    }
    pub fn plan(
        &self,
        source: &str,
        id: u64,
        single: Option<Range<usize>>,
        replacement: &str,
    ) -> Result<Vec<(Range<usize>, String)>, String> {
        if source != self.source {
            return Err("The document changed. Review the candidate again.".into());
        }
        if !valid_replacement(replacement) {
            return Err("Use a token of up to 128 ASCII letters, numbers, underscores or hyphens, starting with a letter and ending with a letter or number.".into());
        }
        let group = self.group(id).ok_or("Candidate is no longer available.")?;
        let ranges = match single {
            Some(range) if group.mentions.contains(&range) => vec![range],
            Some(_) => return Err("Candidate offsets changed. Review it again.".into()),
            None => group.mentions.clone(),
        };
        if ranges.is_empty()
            || ranges
                .iter()
                .any(|range| source.get(range.clone()) != Some(group.original.as_str()))
        {
            return Err("Candidate is no longer available.".into());
        }
        Ok(ranges
            .into_iter()
            .map(|range| (range, replacement.into()))
            .collect())
    }
    pub fn set_replacement(&mut self, id: u64, replacement: &str) {
        if let Some(group) = self.groups.iter_mut().find(|group| group.id == id) {
            group.replacement = replacement.into();
        }
    }
    /// Plan all pending mentions against one source snapshot. Kept candidates
    /// have no mentions; each group retains its proposed or edited replacement.
    pub fn plan_all(
        &self,
        source: &str,
        draft: Option<(u64, &str)>,
    ) -> Result<Vec<(Range<usize>, String)>, String> {
        if source != self.source {
            return Err("The document changed. Review the candidates again.".into());
        }
        let mut edits = Vec::new();
        for group in &self.groups {
            if !group.mentions.is_empty() {
                let replacement = draft
                    .filter(|(id, _)| *id == group.id)
                    .map_or(group.replacement.as_str(), |(_, replacement)| replacement);
                edits.extend(self.plan(source, group.id, None, replacement)?);
            }
        }
        edits.sort_by_key(|(range, _)| range.start);
        if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
            return Err("Candidate offsets overlap. Review the candidates again.".into());
        }
        Ok(edits)
    }
    pub fn keep(&mut self, id: u64, single: Option<Range<usize>>) {
        if let Some(group) = self.groups.iter_mut().find(|group| group.id == id) {
            if let Some(range) = single {
                if group.mentions.contains(&range) {
                    self.exclusions.push(Exclusion {
                        range,
                        original: group.original.clone(),
                    });
                }
            } else {
                group.kept = true;
            }
        }
        let source = self.source.clone();
        self.refresh(&source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn detection(range: Range<usize>, category: Category) -> Detection {
        Detection {
            range,
            category,
            score: 0.9,
        }
    }
    #[test]
    fn unicode_exact_repeats_and_hidden_source_keep_source_syntax() {
        let source = "**Анна** [Анна](https://x.invalid/Анна) `Анна` Аннушка";
        let mut review = Review::default();
        review
            .ingest(source, vec![detection(2..10, Category::Person)])
            .unwrap();
        let group = &review.groups[0];
        assert_eq!(group.mentions.len(), 4);
        let edits = review.plan(source, group.id, None, "PERSON_1").unwrap();
        let mut changed = source.to_string();
        for (range, replacement) in edits.iter().rev() {
            changed.replace_range(range.clone(), replacement);
        }
        assert_eq!(
            changed,
            "**PERSON_1** [PERSON_1](https://x.invalid/PERSON_1) `PERSON_1` Аннушка"
        );
        review.refresh(&changed);
        assert_eq!(review.remaining(), 0);
        review.refresh(source); // undo
        assert_eq!(review.remaining(), 4);
        assert_eq!(review.groups[0].replacement, "PERSON_1");
    }
    #[test]
    fn keep_all_survives_rescan_and_single_keep_rebases() {
        let mut review = Review::default();
        review
            .ingest("Ann Ann", vec![detection(0..3, Category::Person)])
            .unwrap();
        let id = review.groups[0].id;
        review.keep(id, Some(0..3));
        assert_eq!(review.remaining(), 1);
        review.refresh("prefix Ann Ann");
        assert_eq!(review.groups[0].mentions, vec![11..14]);
        review.keep(id, None);
        review
            .ingest("prefix Ann Ann", vec![detection(7..10, Category::Person)])
            .unwrap();
        assert_eq!(review.remaining(), 0);
    }
    #[test]
    fn batch_refresh_preserves_a_kept_mention_between_replacements() {
        let source = "Ann Acme Ann Bob";
        let mut review = Review::default();
        let ann = review.add_manual(source, 0..3, Category::Person).unwrap();
        review
            .add_manual(source, 4..8, Category::Organization)
            .unwrap();
        review.add_manual(source, 13..16, Category::Person).unwrap();
        review.keep(ann, Some(9..12));
        let edits = review.plan_all(source, None).unwrap();
        let mut changed = source.to_owned();
        for (range, replacement) in edits.iter().rev() {
            changed.replace_range(range.clone(), replacement);
        }
        review.refresh_after_edits(&changed, &edits);
        assert_eq!(changed, "PERSON_1 ORG_1 Ann PERSON_2");
        assert_eq!(review.remaining(), 0);
    }
    #[test]
    fn batch_plan_preserves_kept_mentions_custom_tokens_and_hidden_source() {
        let source = "**Анна** Acme Анна [mail](anna@example.invalid) Bob";
        let mut review = Review::default();
        let anna = review.add_manual(source, 2..10, Category::Person).unwrap();
        let acme = source.find("Acme").unwrap();
        let org = review
            .add_manual(source, acme..acme + 4, Category::Organization)
            .unwrap();
        let email = source.find("anna@example.invalid").unwrap();
        review
            .add_manual(source, email..email + 20, Category::Email)
            .unwrap();
        let bob = source.find("Bob").unwrap();
        let kept = review
            .add_manual(source, bob..bob + 3, Category::Person)
            .unwrap();
        review.keep(anna, Some(2..10));
        review.keep(kept, None);
        review.set_replacement(org, "CLIENT_1");
        let edits = review
            .plan_all(source, Some((anna, "PERSON_CUSTOM")))
            .unwrap();
        assert_eq!(edits.len(), 3);
        let mut changed = source.to_owned();
        for (range, replacement) in edits.iter().rev() {
            changed.replace_range(range.clone(), replacement);
        }
        assert_eq!(
            changed,
            "**Анна** CLIENT_1 PERSON_CUSTOM [mail](EMAIL_1) Bob"
        );
        assert!(review.plan_all("changed", None).is_err());
        assert!(
            review
                .plan_all(source, Some((anna, "invalid token")))
                .is_err()
        );
        assert_eq!(review.remaining(), 3);
    }
    #[test]
    fn plans_refuse_stale_or_invalid_ranges_and_syntax_replacements() {
        let mut review = Review::default();
        assert!(
            review
                .ingest("Ё", vec![detection(1..2, Category::Person)])
                .is_err()
        );
        review
            .ingest("Ann", vec![detection(0..3, Category::Person)])
            .unwrap();
        let id = review.groups[0].id;
        assert!(review.plan("Anna", id, None, "PERSON_1").is_err());
        assert!(review.plan("Ann", id, Some(1..3), "PERSON_1").is_err());
        assert!(review.plan("Ann", id, None, "](bad)").is_err());
        assert!(
            review
                .add_manual("[Ann](url)", 0..10, Category::Person)
                .is_err()
        );
    }
    #[test]
    fn hidden_values_are_replaceable_but_html_names_quotes_and_list_prefixes_are_protected() {
        let source = "1. Anna\n\n[Anna](https://x.invalid/Anna) ![photo](Anna.png) <Anna Anna=\"Anna\">Anna</Anna>";
        let mut review = Review::default();
        let id = review.add_manual(source, 3..7, Category::Person).unwrap();
        let group = review.group(id).unwrap();
        assert_eq!(group.mentions.len(), 6);
        let mut changed = source.to_owned();
        for (range, replacement) in review
            .plan(source, id, None, "PERSON_1")
            .unwrap()
            .iter()
            .rev()
        {
            changed.replace_range(range.clone(), replacement);
        }
        assert_eq!(
            changed,
            "1. PERSON_1\n\n[PERSON_1](https://x.invalid/PERSON_1) ![photo](PERSON_1.png) <Anna Anna=\"PERSON_1\">PERSON_1</Anna>"
        );
        assert!(review.add_manual(source, 0..2, Category::Identity).is_err());
        let tag = source.find("<Anna").unwrap() + 1;
        assert!(
            review
                .add_manual(source, tag..tag + 4, Category::Person)
                .is_err()
        );
        let quote = source.find("\"Anna\"").unwrap();
        assert!(
            review
                .add_manual(source, quote + 1..quote + 6, Category::Person)
                .is_err()
        );
        assert!(!valid_replacement("---"));
        assert!(!valid_replacement("1"));
        assert!(!valid_replacement("PERSON_"));
    }
    #[test]
    fn parenthesized_phone_values_do_not_consume_link_delimiters() {
        let source = "Call (202) 555-0101. [contact](tel:(202)555-0101)";
        let mut review = Review::default();
        let id = review.add_manual(source, 5..19, Category::Phone).unwrap();
        assert_eq!(
            review.plan(source, id, None, "PHONE_1").unwrap()[0].0,
            5..19
        );
        let end = source.len();
        assert!(
            review
                .add_manual(source, end - 13..end, Category::Phone)
                .is_err()
        );
    }
    #[test]
    fn mappings_link_variants_explicitly_and_never_collide_with_source_tokens() {
        let mut review = Review::default();
        let source = "PERSON_1 Анна Анны";
        let a = review.add_manual(source, 9..17, Category::Person).unwrap();
        let b = review.add_manual(source, 18..26, Category::Person).unwrap();
        assert_eq!(review.group(a).unwrap().replacement, "PERSON_2");
        review.set_replacement(b, "PERSON_2");
        assert_eq!(
            review.group(a).unwrap().replacement,
            review.group(b).unwrap().replacement
        );
    }
}
