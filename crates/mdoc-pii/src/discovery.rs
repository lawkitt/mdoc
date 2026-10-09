//! Immutable, cancellable candidate discovery; reusable cached matcher.
use super::{
    Category, exact_boundary, intersects, interval_conflict,
    syntax::{plain_text_span, protected_syntax},
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
            .filter(|v| {
                v.split_once('_').is_some_and(|(prefix, suffix)| {
                    !suffix.is_empty()
                        && suffix.bytes().all(|b| b.is_ascii_digit())
                        && Category::ALL.iter().any(|c| c.token() == prefix)
                })
            })
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
        let matcher = self.matcher.unwrap_or_else(|| {
            Arc::new(
                AhoCorasickBuilder::new()
                    .kind(Some(AhoCorasickKind::ContiguousNFA))
                    .build(self.originals.iter().map(|s| s.as_ref()))
                    .expect("bounded nonempty seeds"),
            )
        });
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let protected = protected_syntax(source);
        let mut hits = Vec::new();
        for (index, hit) in matcher.find_overlapping_iter(source).enumerate() {
            if index % 1024 == 0 && cancel.load(Ordering::Relaxed) {
                return None;
            }
            hits.push((hit.pattern().as_usize(), hit.start()..hit.end()));
        }
        // Higher-priority (manual) originals claim overlaps first, so a manual
        // "Ivan Petrov" supersedes a pending "Ivan" inside it.
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
            if !self.enabled[index]
                || self.excluded.contains(&(range.start, range.end))
                || !exact_boundary(source, &range, &self.originals[index])
                || intersects(&protected, &range)
                || !plain_text_span(source, range.clone())
                || (if single {
                    range.start < last_end
                } else {
                    interval_conflict(&occupied, &range)
                })
            {
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
