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
            for alias in std::iter::once(&identity.alias).chain(&identity.previous) {
                match parse_alias(alias) {
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
        tokens.sort_by_key(|t| (t.range.start, std::cmp::Reverse(t.range.end)));
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
                }
                None => unresolved += 1,
            }
        }
        Restoration {
            restored: edits.len(),
            values: values.len(),
            unresolved,
            edits,
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
    /// Restored mentions.
    pub restored: usize,
    /// Distinct identities among them.
    pub values: usize,
    pub unresolved: usize,
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
                Some(ExportedIdentity {
                    alias: identity.alias.clone(),
                    previous: entry.aliases,
                    category: identity.category,
                    custom: identity.custom_alias,
                    originals: entry.originals,
                })
            })
            .collect()
    }
    /// Start this (empty) review with another document's identities: each
    /// original becomes a known wording proposed with its existing alias.
    pub fn seed(&mut self, identities: &[ExportedIdentity], source: &str) {
        for exported in identities {
            self.next_id += 1;
            let id = self.next_id;
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
    }
}

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;
