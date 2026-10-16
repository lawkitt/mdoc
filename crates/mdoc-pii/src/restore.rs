//! Restoring originals into text returned by an external AI (ADR 0041). The
//! AI often reshapes placeholders, so generated aliases match leniently; an
//! alias-shaped token that matches nothing is reported, never guessed.
use super::{Category, Review};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

/// One identity as another document needs it: its aliases and original wordings.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportedIdentity {
    pub alias: String,
    /// Earlier aliases still present in applied text (e.g. before a rename).
    pub previous: Vec<String>,
    pub category: Category,
    pub custom: bool,
    /// The primary wording first: the one an alias restores to.
    pub originals: Vec<Arc<str>>,
}
impl ExportedIdentity {
    pub fn wording(&self) -> &str {
        &self.originals[0]
    }
}

/// `PERSON_3` → (Person, 3). Only the canonical shape: uppercase token, `_`, digits.
pub(crate) fn parse_alias(value: &str) -> Option<(Category, u64)> {
    let (prefix, digits) = value.split_once('_')?;
    let category = *Category::ALL.iter().find(|c| c.token() == prefix)?;
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((category, digits.parse().ok()?))
}

/// Alias lookup for one source document's identities.
#[derive(Clone, Debug, Default)]
pub struct RestoreMap {
    identities: Vec<ExportedIdentity>,
    generated: HashMap<(Category, u64), usize>,
    custom: Vec<(String, usize)>,
}
impl RestoreMap {
    pub fn new(identities: Vec<ExportedIdentity>) -> Self {
        let mut generated = HashMap::new();
        let mut custom = Vec::new();
        for (index, identity) in identities.iter().enumerate() {
            for (current, alias) in std::iter::once((true, &identity.alias))
                .chain(identity.previous.iter().map(|p| (false, p)))
            {
                // A custom alias matches exactly, even one that looks generated.
                let custom_alias = current && identity.custom;
                match parse_alias(alias).filter(|_| !custom_alias) {
                    Some(key) => {
                        generated.entry(key).or_insert(index);
                    }
                    None => custom.push((alias.clone(), index)),
                }
            }
        }
        // Longer custom aliases win where one contains another.
        custom.sort_by_key(|(alias, _)| std::cmp::Reverse(alias.len()));
        Self {
            identities,
            generated,
            custom,
        }
    }
    pub fn identities(&self) -> &[ExportedIdentity] {
        &self.identities
    }
    pub fn is_empty(&self) -> bool {
        self.identities.is_empty()
    }
    /// Alias-shaped tokens in `text`, in source order and non-overlapping.
    pub fn tokens(&self, text: &str) -> Vec<Token> {
        let mut tokens = generated_tokens(text)
            .into_iter()
            .filter_map(|(range, key, strict)| {
                let identity = self.generated.get(&key).copied();
                // Loose forms ("Tax 2023", "date-1") are only placeholders
                // when they name a known identity.
                (identity.is_some() || strict).then_some(Token { range, identity })
            })
            .collect::<Vec<_>>();
        for (alias, index) in &self.custom {
            for (start, _) in text.match_indices(alias.as_str()) {
                let range = start..start + alias.len();
                if word_start(text, start) && word_end(text, range.end) {
                    tokens.push(Token {
                        range,
                        identity: Some(*index),
                    });
                }
            }
        }
        // Where two readings cover the same span, the one naming an identity wins.
        tokens.sort_by_key(|t| {
            (
                t.range.start,
                std::cmp::Reverse(t.range.end),
                t.identity.is_none(),
            )
        });
        let mut end = 0;
        tokens.retain(|t| {
            let keep = t.range.start >= end;
            if keep {
                end = t.range.end;
            }
            keep
        });
        tokens
    }
    /// Replace every known alias in `text` with its identity's primary wording.
    pub fn restore(&self, text: &str) -> Restoration {
        let mut edits = Vec::new();
        let mut sources = Vec::new();
        let mut values = HashSet::new();
        let mut unresolved = 0;
        for token in self.tokens(text) {
            match token.identity {
                Some(index) => {
                    values.insert(index);
                    let mut wording = self.identities[index].wording();
                    // A wording detected with its full stop ("Ltd.") does not
                    // double the one the reply already has after the alias.
                    if wording.ends_with('.') && text[token.range.end..].starts_with('.') {
                        wording = &wording[..wording.len() - 1];
                    }
                    edits.push((token.range, wording.to_string()));
                    sources.push(index);
                }
                None => unresolved += 1,
            }
        }
        Restoration {
            restored: edits.len(),
            values: values.len(),
            unresolved,
            edits,
            sources,
        }
    }
    /// Identity indices to offer for an unresolved `token`: those of the
    /// category it names first (`PERSON_9` offers people first).
    pub fn choices(&self, token: &str) -> Vec<usize> {
        let category = generated_tokens(token)
            .first()
            .map(|(_, (category, _), _)| *category);
        let mut choices: Vec<_> = (0..self.identities.len()).collect();
        choices.sort_by_key(|&i| Some(self.identities[i].category) != category);
        choices
    }
    /// The reverse of [`Self::restore`]: replace every known original in
    /// `text` with its identity's alias (exact wording, whole words, longest
    /// first). Returns the text and how many names were replaced. Names this
    /// map doesn't know stay as they are; no detection runs.
    pub fn pseudonymize(&self, text: &str) -> (String, usize) {
        let mut patterns = Vec::new();
        let mut owners = Vec::new();
        for (index, identity) in self.identities.iter().enumerate() {
            for original in &identity.originals {
                if !original.trim().is_empty() {
                    patterns.push(original.as_ref());
                    owners.push(index);
                }
            }
        }
        if patterns.is_empty() {
            return (text.to_string(), 0);
        }
        let Ok(matcher) = aho_corasick::AhoCorasick::new(&patterns) else {
            return (text.to_string(), 0);
        };
        // Whole words first, then the leftmost and longest of those: a longer
        // match that is not a whole word must not hide a shorter one that is.
        let mut found: Vec<_> = matcher
            .find_overlapping_iter(text)
            .map(|m| (m.start()..m.end(), owners[m.pattern().as_usize()]))
            .filter(|(range, _)| super::exact_boundary(text, range, &text[range.clone()]))
            .collect();
        found.sort_by_key(|(r, _)| (r.start, std::cmp::Reverse(r.end)));
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        let mut count = 0;
        for (range, owner) in found {
            if range.start < last {
                continue;
            }
            out.push_str(&text[last..range.start]);
            out.push_str(&self.identities[owner].alias);
            last = range.end;
            count += 1;
        }
        out.push_str(&text[last..]);
        (out, count)
    }
    /// Alias-shaped tokens that name no identity of this map.
    pub fn unresolved(&self, text: &str) -> Vec<Range<usize>> {
        self.tokens(text)
            .into_iter()
            .filter(|t| t.identity.is_none())
            .map(|t| t.range)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub range: Range<usize>,
    /// Index into [`RestoreMap::identities`]; `None` for an unresolved token.
    pub identity: Option<usize>,
}

/// Edits to apply in one batch, with counts for the summary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Restoration {
    pub edits: Vec<(Range<usize>, String)>,
    /// The identity index each edit restores, in edit order.
    pub sources: Vec<usize>,
    /// Restored mentions.
    pub restored: usize,
    /// Distinct identities among them.
    pub values: usize,
    pub unresolved: usize,
}

impl Restoration {
    /// Where each restored wording lands once the edits are applied, with the
    /// identity index it restores.
    pub fn restored_ranges(&self) -> Vec<(Range<usize>, usize)> {
        let mut delta = 0isize;
        self.edits
            .iter()
            .zip(&self.sources)
            .map(|((range, value), &source)| {
                let start = range.start.saturating_add_signed(delta);
                delta += value.len() as isize - range.len() as isize;
                (start..start + value.len(), source)
            })
            .collect()
    }
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
fn word_start(text: &str, at: usize) -> bool {
    !text[..at].chars().next_back().is_some_and(word_char)
}
/// A token ends at a non-word character, or where Cyrillic letters are glued
/// on as an inflection (`PERSON_1у`). Latin letters continue the word.
fn word_end(text: &str, at: usize) -> bool {
    text[at..]
        .chars()
        .next()
        .is_none_or(|c| !word_char(c) || is_cyrillic(c))
}
fn is_cyrillic(c: char) -> bool {
    matches!(c, '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}')
}

/// Generated-alias candidates: a category token in any case, one separator
/// (`_`, `-`, space or no-break space) and up to nine digits. `strict` marks the
/// canonical look (uppercase, `_` or `-`) that is reported even when unknown.
fn generated_tokens(text: &str) -> Vec<(Range<usize>, (Category, u64), bool)> {
    // Longest first, so REDACTED is not read as a shorter token.
    let mut categories = Category::ALL;
    categories.sort_by_key(|c| std::cmp::Reverse(c.token().len()));
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if !bytes[at].is_ascii_alphabetic() || !word_start(text, at) {
            at += 1;
            continue;
        }
        let mut matched = None;
        for category in categories {
            let token = category.token();
            let Some(prefix) = text.get(at..at + token.len()) else {
                continue;
            };
            if !prefix.eq_ignore_ascii_case(token) {
                continue;
            }
            let rest = &text[at + token.len()..];
            let Some(separator) = rest.chars().next() else {
                continue;
            };
            if !matches!(separator, '_' | '-' | ' ' | '\u{a0}') {
                continue;
            }
            let digits_start = at + token.len() + separator.len_utf8();
            let digits = text[digits_start..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            let end = digits_start + digits;
            if digits == 0 || digits > 9 || !word_end(text, end) {
                continue;
            }
            let number = text[digits_start..end].parse().unwrap_or(u64::MAX);
            let strict = prefix == token && matches!(separator, '_' | '-');
            matched = Some((at..end, (category, number), strict));
            break;
        }
        match matched {
            Some(token) => {
                at = token.0.end;
                found.push(token);
            }
            None => at += 1,
        }
    }
    found
}

impl Review {
    /// This document's identities with the aliases its text contains, for
    /// restoring AI output and seeding another document's review.
    pub fn export_identities(&self) -> Vec<ExportedIdentity> {
        self.export_identities_with_ids()
            .into_iter()
            .map(|(_, e)| e)
            .collect()
    }
    /// [`Self::export_identities`] with each identity's id in this review.
    pub fn export_identities_with_ids(&self) -> Vec<(u64, ExportedIdentity)> {
        #[derive(Default)]
        struct Entry {
            aliases: Vec<String>,
            originals: Vec<Arc<str>>,
        }
        fn push<T: PartialEq>(list: &mut Vec<T>, value: T) {
            if !list.contains(&value) {
                list.push(value);
            }
        }
        let active: HashSet<u64> = self.active_identities().into_iter().collect();
        let mut entries: BTreeMap<u64, Entry> = BTreeMap::new();
        for applied in self.applied() {
            if applied.step.identity == 0 {
                continue;
            }
            let entry = entries.entry(applied.step.identity).or_default();
            push(&mut entry.aliases, applied.step.after.to_string());
            push(&mut entry.originals, applied.step.original_shared().clone());
        }
        for variant in self.variants() {
            if variant.kept {
                continue;
            }
            if let Some(id) = self
                .variant_identity(variant.id)
                .filter(|id| active.contains(id))
            {
                push(
                    &mut entries.entry(id).or_default().originals,
                    variant.original.clone(),
                );
            }
        }
        entries
            .into_iter()
            .filter_map(|(id, mut entry)| {
                let identity = self.identity(id)?;
                if entry.originals.is_empty() {
                    return None;
                }
                // The primary wording is the fullest form: most characters,
                // then first seen.
                let primary = entry
                    .originals
                    .iter()
                    .enumerate()
                    .max_by_key(|(i, o)| (o.chars().count(), std::cmp::Reverse(*i)))
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                entry.originals.swap(0, primary);
                let wording = entry.originals.remove(0);
                entry
                    .originals
                    .sort_by_key(|o| std::cmp::Reverse(o.chars().count()));
                entry.originals.insert(0, wording);
                entry.aliases.retain(|a| *a != identity.alias);
                Some((
                    id,
                    ExportedIdentity {
                        alias: identity.alias.clone(),
                        previous: entry.aliases,
                        category: identity.category,
                        custom: identity.custom_alias,
                        originals: entry.originals,
                    },
                ))
            })
            .collect()
    }
    /// Start this (empty) review with another document's identities: each
    /// original becomes a known wording proposed with its existing alias.
    /// Returns the new identity ids, in the order of `identities`.
    pub fn seed(&mut self, identities: &[ExportedIdentity], source: &str) -> Vec<u64> {
        self.revision.advance();
        let mut ids = Vec::with_capacity(identities.len());
        for exported in identities {
            self.next_id += 1;
            let id = self.next_id;
            ids.push(id);
            self.identities.add(super::Identity {
                id,
                alias: exported.alias.clone(),
                category: exported.category,
                owner: None,
                custom_alias: exported.custom,
            });
            for alias in std::iter::once(&exported.alias).chain(&exported.previous) {
                self.occupied_tokens.insert(alias.clone());
                if let Some((category, number)) = parse_alias(alias) {
                    let counter = self.counters.entry(category).or_default();
                    *counter = (*counter).max(number as usize);
                }
            }
            for original in &exported.originals {
                if self.candidates.by_original(original).is_some() {
                    continue;
                }
                self.next_id += 1;
                self.candidates.push(super::Variant {
                    id: self.next_id,
                    original: original.clone(),
                    identity: id,
                    category: exported.category,
                    replacement: exported.alias.clone(),
                    mentions: Vec::new(),
                    kept: false,
                    keep_default: false,
                    priority: 0,
                });
                self.identities
                    .remember_normalized(original, exported.category, id);
            }
        }
        self.refresh(source);
        ids
    }
    /// Treat alias tokens already in `source` as applied replacements of the
    /// seeded identities `ids`, so a reopened document can restore them. Only
    /// exact current or earlier aliases count; reshaped tokens are left alone.
    /// Recorded against editor history state `history`. Returns the count.
    pub fn relink(
        &mut self,
        source: &str,
        identities: &[ExportedIdentity],
        ids: &[u64],
        history: u64,
    ) -> usize {
        self.revision.advance();
        let map = RestoreMap::new(identities.to_vec());
        let plans: Vec<_> = map
            .tokens(source)
            .into_iter()
            .filter_map(|token| {
                let index = token.identity?;
                let exported = &identities[index];
                let text = &source[token.range.clone()];
                (text == exported.alias || exported.previous.iter().any(|p| p == text)).then(|| {
                    super::tracking::ReplacementPlan {
                        range: token.range,
                        before: Arc::from(exported.wording()),
                        after: Arc::from(text),
                        category: exported.category,
                        identity: ids[index],
                    }
                })
            })
            .filter(|plan| self.tracking.at(&plan.range).is_none())
            .collect();
        let added = self.tracking.prepare(&plans);
        let count = added.len();
        self.tracking.commit(history, added);
        count
    }
}

/// How a document joining a case lines up with it (ADR 0041 #10).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JoinPlan {
    /// Identity id → alias, category and custom flag it takes.
    pub changes: Vec<(u64, String, Category, bool)>,
    /// Case identities the document lacks; seed them so their originals are
    /// proposed with the case aliases.
    pub unmatched: Vec<ExportedIdentity>,
}

/// Align a document's identities with a case: an identity sharing an original
/// with a case identity takes its alias; any other whose alias the case already
/// uses takes the next free number. Aliases are never reused.
pub fn plan_join(case: &[ExportedIdentity], document: &[(u64, ExportedIdentity)]) -> JoinPlan {
    let names = |e: &ExportedIdentity| {
        std::iter::once(e.alias.clone())
            .chain(e.previous.iter().cloned())
            .collect::<Vec<_>>()
    };
    // Aliases a document identity may not keep: the case's, and other
    // identities' earlier ones in this document.
    let case_names: HashSet<String> = case.iter().flat_map(names).collect();
    // Aliases a fresh number may not take: every alias either side ever used.
    let mut taken: HashSet<String> = case_names
        .iter()
        .cloned()
        .chain(document.iter().flat_map(|(_, d)| names(d)))
        .collect();
    let mut counters: HashMap<Category, u64> = HashMap::new();
    for alias in &taken {
        if let Some((category, number)) = parse_alias(alias) {
            let counter = counters.entry(category).or_default();
            *counter = (*counter).max(number);
        }
    }
    let mut matched = HashSet::new();
    let mut plan = JoinPlan::default();
    let mut assigned: Vec<(u64, String, Category, bool)> = Vec::new();
    for (id, doc) in document {
        let found = case.iter().position(|c| {
            !matched.contains(&std::ptr::from_ref(c))
                && c.originals.iter().any(|o| doc.originals.contains(o))
        });
        let target = match found {
            Some(index) => {
                let c = &case[index];
                matched.insert(std::ptr::from_ref(c));
                (c.alias.clone(), c.category, c.custom)
            }
            None if case_names.contains(&doc.alias)
                || assigned.iter().any(|a| a.1 == doc.alias)
                || document
                    .iter()
                    .any(|(other, d)| other != id && d.previous.contains(&doc.alias)) =>
            {
                let counter = counters.entry(doc.category).or_default();
                let alias = loop {
                    *counter += 1;
                    let alias = format!("{}_{counter}", doc.category.token());
                    if !taken.contains(&alias) && !assigned.iter().any(|a| a.1 == alias) {
                        break alias;
                    }
                };
                (alias, doc.category, false)
            }
            None => (doc.alias.clone(), doc.category, doc.custom),
        };
        taken.insert(target.0.clone());
        assigned.push((*id, target.0, target.1, target.2));
    }
    plan.changes = assigned
        .into_iter()
        .filter(|(id, alias, category, _)| {
            document
                .iter()
                .find(|(d, _)| d == id)
                .is_some_and(|(_, d)| d.alias != *alias || d.category != *category)
        })
        .collect();
    plan.unmatched = case
        .iter()
        .filter(|c| !matched.contains(&std::ptr::from_ref(*c)))
        .cloned()
        .collect();
    plan
}

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;
