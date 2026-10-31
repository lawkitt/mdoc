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
        // Current aliases first: an alias that is one identity's own names it,
        // never another identity that used it before (a join can hand a
        // document's old alias to a different case identity).
        let current = identities
            .iter()
            .enumerate()
            .map(|(index, identity)| (true, &identity.alias, index));
        let previous = identities
            .iter()
            .enumerate()
            .flat_map(|(index, identity)| identity.previous.iter().map(move |p| (false, p, index)));
        for (current, alias, index) in current.chain(previous) {
            // A custom alias matches exactly, even one that looks generated.
            let custom_alias = current && identities[index].custom;
            match parse_alias(alias).filter(|_| !custom_alias) {
                Some(key) => {
                    generated.entry(key).or_insert(index);
                }
                None if custom.iter().any(|(a, _)| a == alias) => {}
                None => custom.push((alias.clone(), index)),
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
        let found = self.originals_in(text);
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for (range, owner) in &found {
            out.push_str(&text[last..range.start]);
            out.push_str(&self.identities[*owner].alias);
            last = range.end;
        }
        out.push_str(&text[last..]);
        (out, found.len())
    }
    /// How many names of this map still appear in `text`: by an original
    /// wording (exact, whole words), or by a lone word of a multi-word
    /// person original ("Марина" of "Марина Сергеевна"): names a
    /// pseudonymized text did not replace (ADR 0084). Counted by alias, as
    /// a tab's mapping and its case list one person twice.
    pub fn known_names(&self, text: &str) -> usize {
        let whole = self.originals_in(text);
        // A word inside a whole original or an alias is no leftover of its
        // own: a custom alias may share a word with an original.
        let taken: Vec<Range<usize>> = whole
            .iter()
            .map(|(range, _)| range.clone())
            .chain(self.tokens(text).into_iter().map(|token| token.range))
            .collect();
        let words = self
            .name_words_in(text)
            .into_iter()
            .filter(|(range, _)| {
                !taken
                    .iter()
                    .any(|t| t.start < range.end && range.start < t.end)
            });
        whole
            .into_iter()
            .chain(words)
            .map(|(_, owner)| self.identities[owner].alias.as_str())
            .collect::<HashSet<_>>()
            .len()
    }
    /// Lone words of multi-word person originals in `text`, exact and
    /// whole, with the first identity that has each. Only name-like words:
    /// 3+ letters, not starting lowercase, no courtesy title (ADR 0068), so
    /// "Ms", "van" and initials never count.
    fn name_words_in(&self, text: &str) -> Vec<(Range<usize>, usize)> {
        let mut patterns: Vec<&str> = Vec::new();
        let mut owners = Vec::new();
        for (index, identity) in self.identities.iter().enumerate() {
            if identity.category != Category::Person {
                continue;
            }
            for original in &identity.originals {
                let words: Vec<&str> = original
                    .split_whitespace()
                    .map(|w| w.trim_matches(|c: char| !c.is_alphabetic()))
                    .filter(|w| {
                        !w.is_empty()
                            && !super::identities::TITLES.contains(&w.to_lowercase().as_str())
                    })
                    .collect();
                if words.len() < 2 {
                    continue;
                }
                for word in words {
                    let name_like = word.chars().filter(|c| c.is_alphabetic()).count() >= 3
                        && !word.starts_with(char::is_lowercase);
                    if name_like && !patterns.contains(&word) {
                        patterns.push(word);
                        owners.push(index);
                    }
                }
            }
        }
        if patterns.is_empty() {
            return Vec::new();
        }
        let Ok(matcher) = aho_corasick::AhoCorasick::new(&patterns) else {
            return Vec::new();
        };
        matcher
            .find_overlapping_iter(text)
            .map(|m| (m.start()..m.end(), owners[m.pattern().as_usize()]))
            .filter(|(range, _)| super::exact_boundary(text, range, &text[range.clone()]))
            .collect()
    }
    /// Known originals in `text` with their identity index: exact wording,
    /// whole words, then the leftmost and longest of those, non-overlapping.
    fn originals_in(&self, text: &str) -> Vec<(Range<usize>, usize)> {
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
            return Vec::new();
        }
        let Ok(matcher) = aho_corasick::AhoCorasick::new(&patterns) else {
            return Vec::new();
        };
        // Whole words first, then the leftmost and longest of those: a longer
        // match that is not a whole word must not hide a shorter one that is.
        let mut found: Vec<_> = matcher
            .find_overlapping_iter(text)
            .map(|m| (m.start()..m.end(), owners[m.pattern().as_usize()]))
            .filter(|(range, _)| super::exact_boundary(text, range, &text[range.clone()]))
            .collect();
        found.sort_by_key(|(r, _)| (r.start, std::cmp::Reverse(r.end)));
        let mut last = 0;
        found.retain(|(range, _)| {
            let keep = range.start >= last;
            if keep {
                last = range.end;
            }
            keep
        });
        found
    }
    /// Alias tokens of `text` written exactly as an identity's current or
    /// earlier alias, in source order: the ones a reopened document relinks
    /// as applied (see `Review::relink`), so Copy for AI names the same.
    pub fn exact_aliases<'t>(&self, text: &'t str) -> Vec<&'t str> {
        self.tokens(text)
            .into_iter()
            .filter_map(|token| {
                let identity = &self.identities[token.identity?];
                let written = &text[token.range];
                (written == identity.alias || identity.previous.iter().any(|p| p == written))
                    .then_some(written)
            })
            .collect()
    }
    /// Alias-shaped tokens that name no identity of this map.
    pub fn unresolved(&self, text: &str) -> Vec<Range<usize>> {
        self.tokens(text)
            .into_iter()
            .filter(|t| t.identity.is_none())
            .map(|t| t.range)
            .collect()
    }
    /// [`Self::restore`] applied to `text` for reading in place (ADR 0078):
    /// the restored text, where each restored wording landed and where each
    /// unresolved token sits in it. Same matching as the restore tab.
    pub fn restore_in_place(&self, text: &str) -> RestoredText {
        let restoration = self.restore(text);
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        for (range, value) in &restoration.edits {
            out.push_str(&text[at..range.start]);
            out.push_str(value);
            at = range.end;
        }
        out.push_str(&text[at..]);
        // An unresolved token moves by the length change of the edits before it.
        let mut edits = restoration.edits.iter().peekable();
        let mut delta = 0isize;
        let unresolved = self
            .unresolved(text)
            .into_iter()
            .map(|range| {
                while let Some((edit, value)) = edits.next_if(|(e, _)| e.end <= range.start) {
                    delta += value.len() as isize - edit.len() as isize;
                }
                let start = range.start.saturating_add_signed(delta);
                start..start + range.len()
            })
            .collect();
        RestoredText {
            restored: restoration.restored_ranges(),
            values: restoration.values,
            text: out,
            unresolved,
        }
    }
    /// Rewrite `text`'s alias tokens so they keep naming the same identities
    /// after their aliases change (ADR 0078): a token naming identity `i` of
    /// this map takes `aliases[i]` when that is `Some`. All tokens are read
    /// against this map first, so swapped aliases swap back correctly. A
    /// reshaped generated token (`person 1`) keeps its shape and takes the
    /// new number when the category stays; any other token becomes the new
    /// alias. Returns `None` when nothing changed.
    pub fn realias(&self, text: &str, aliases: &[Option<String>]) -> Option<String> {
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        for token in self.tokens(text) {
            let Some(alias) = token
                .identity
                .and_then(|index| aliases.get(index))
                .and_then(Option::as_deref)
            else {
                continue;
            };
            let old = &text[token.range.clone()];
            let new = match (generated_tokens(old).first(), parse_alias(alias)) {
                (Some((range, (category, _), _)), Some((new_category, number)))
                    if range.len() == old.len() && *category == new_category =>
                {
                    let digits = old.bytes().rev().take_while(u8::is_ascii_digit).count();
                    format!("{}{number}", &old[..old.len() - digits])
                }
                _ => alias.to_string(),
            };
            if new != old {
                out.push_str(&text[at..token.range.start]);
                out.push_str(&new);
                at = token.range.end;
            }
        }
        (at > 0).then(|| {
            out.push_str(&text[at..]);
            out
        })
    }
}

/// A reply restored for reading in place (ADR 0078).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RestoredText {
    pub text: String,
    /// Each restored wording's span in `text`, with its identity index.
    pub restored: Vec<(Range<usize>, usize)>,
    /// Distinct identities restored ([`Restoration::values`]).
    pub values: usize,
    /// Unresolved alias-shaped tokens' spans in `text`, in order.
    pub unresolved: Vec<Range<usize>>,
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
    /// Reserve retired case aliases without retaining any original wording.
    /// Generated counters are monotonic; custom reservations survive discovery.
    pub fn reserve_aliases(&mut self, aliases: &[String]) {
        self.revision.advance();
        for alias in aliases {
            self.reserved_tokens.insert(alias.clone());
            self.occupied_tokens.insert(alias.clone());
            if let Some((category, number)) = parse_alias(alias) {
                let counter = self.counters.entry(category).or_default();
                *counter = (*counter).max(number as usize);
            }
        }
    }
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
        // A homonym's explicitly assigned occurrence can outlive its group's
        // default identity, e.g. when that identity was forgotten from a case.
        for candidate in self.candidates() {
            if let Some(id) = self.occurrence_identity(candidate.variant, &candidate.range)
                && active.contains(&id)
                && let Some(variant) = self.variant(candidate.variant)
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
                // then the saved or merge target's primary, then first seen.
                // The pin only breaks ties, so a longer wording linked later
                // still becomes primary.
                let pinned = self.identities.primary_wording(id);
                let primary = entry
                    .originals
                    .iter()
                    .enumerate()
                    .max_by_key(|(i, o)| {
                        (o.chars().count(), pinned == Some(o), std::cmp::Reverse(*i))
                    })
                    .map_or(0, |(i, _)| i);
                entry.originals.swap(0, primary);
                let wording = entry.originals.remove(0);
                entry
                    .originals
                    .sort_by_key(|o| std::cmp::Reverse(o.chars().count()));
                entry.originals.insert(0, wording);
                for alias in self.identities.retired_aliases(id) {
                    push(&mut entry.aliases, alias.clone());
                }
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
        self.seed_with(identities, source, false)
    }
    /// [`Self::seed`] as part of the undo step being recorded: its new
    /// proposals are switched on in the identity policy, so undoing the step
    /// (to a policy from before it) withdraws them, as with a manual
    /// addition. The caller commits the policy to that step.
    pub fn seed_on_step(&mut self, identities: &[ExportedIdentity], source: &str) -> Vec<u64> {
        self.seed_with(identities, source, true)
    }
    fn seed_with(
        &mut self,
        identities: &[ExportedIdentity],
        source: &str,
        on_step: bool,
    ) -> Vec<u64> {
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
            self.identities
                .seed_restoration(id, &exported.previous, exported.originals.first());
            for alias in std::iter::once(&exported.alias).chain(&exported.previous) {
                self.occupied_tokens.insert(alias.clone());
                if let Some((category, number)) = parse_alias(alias) {
                    let counter = self.counters.entry(category).or_default();
                    *counter = (*counter).max(number as usize);
                }
            }
            for original in &exported.originals {
                if let Some(group) = self.candidates.by_original(original) {
                    // A proposal an undone join withdrew returns on this
                    // step, for the identity seeded now; a Keep stays.
                    if on_step && self.withdrawn(group) {
                        self.set_kept(group, false);
                        let _ = self.assign_variant(group, id);
                    }
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
                    keep_default: on_step,
                    priority: 0,
                    detected: false,
                });
                if on_step {
                    self.set_kept(self.next_id, false);
                }
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
pub fn plan_join(
    case: &[ExportedIdentity],
    document: &[(u64, ExportedIdentity)],
    reserved: &[String],
) -> JoinPlan {
    let names = |e: &ExportedIdentity| {
        std::iter::once(e.alias.clone())
            .chain(e.previous.iter().cloned())
            .collect::<Vec<_>>()
    };
    // Aliases a document identity may not keep: the case's, and other
    // identities' earlier ones in this document.
    let case_names: HashSet<String> = case
        .iter()
        .flat_map(names)
        .chain(reserved.iter().cloned())
        .collect();
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
                || doc.previous.iter().any(|alias| case_names.contains(alias))
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
