//! A pattern of one regex or several searched as one alternation: the
//! regex crate's size limit is per regex, so a pattern near it is written
//! as a list of its forms (`pattern = ['…', '…']`), each compiled alone.
use regex::{Captures, Regex};

/// One regex or several that find what one alternation of them would, in
/// their order (`A|B|…`): a search finds the leftmost match of any of
/// them, and of those starting there the first regex's.
pub(super) struct Alternation(pub(super) Vec<Regex>);

impl Alternation {
    /// The first match at or after `start`, as [`Regex::captures_at`]
    /// finds it.
    pub(super) fn captures_at<'h>(&self, haystack: &'h str, start: usize) -> Option<Captures<'h>> {
        let mut first: Option<Captures<'h>> = None;
        for regex in &self.0 {
            if let Some(found) = regex.captures_at(haystack, start)
                && first
                    .as_ref()
                    .is_none_or(|first| match_start(&found) < match_start(first))
            {
                first = Some(found);
            }
        }
        first
    }

    /// The matches in `haystack`, each searched for after the last, as
    /// [`Regex::captures_iter`] finds them: an empty match where the last
    /// match ended is skipped, and the search goes on a character later.
    pub(super) fn captures_iter<'h>(
        &'h self,
        haystack: &'h str,
    ) -> Box<dyn Iterator<Item = Captures<'h>> + 'h> {
        if let [regex] = self.0.as_slice() {
            return Box::new(regex.captures_iter(haystack));
        }
        let mut at = 0;
        let mut last_end = None;
        Box::new(std::iter::from_fn(move || {
            while at <= haystack.len() {
                let found = self.captures_at(haystack, at)?;
                let whole = found.get(0).expect("group 0 is the match");
                if whole.is_empty() && last_end == Some(whole.end()) {
                    at = whole.end()
                        + haystack[whole.end()..]
                            .chars()
                            .next()
                            .map_or(1, char::len_utf8);
                    continue;
                }
                at = whole.end();
                last_end = Some(whole.end());
                return Some(found);
            }
            None
        }))
    }

    /// The regexes' group names, each once, in order: a group of one
    /// regex is no match's in the others.
    pub(super) fn capture_names(&self) -> impl Iterator<Item = Option<&str>> {
        let mut seen = std::collections::HashSet::new();
        self.0
            .iter()
            .flat_map(Regex::capture_names)
            .filter(move |name| name.is_none_or(|name| seen.insert(name)))
    }
}

/// Where a match starts.
fn match_start(found: &Captures<'_>) -> usize {
    found.get(0).expect("group 0 is the match").start()
}

/// What [`super::spans::span_ranges`] searches with: a regex, or an
/// alternation of them.
pub(super) trait Search {
    fn captures_at<'h>(&self, haystack: &'h str, start: usize) -> Option<Captures<'h>>;
    fn capture_names(&self) -> impl Iterator<Item = Option<&str>>;
}

impl Search for Regex {
    fn captures_at<'h>(&self, haystack: &'h str, start: usize) -> Option<Captures<'h>> {
        Regex::captures_at(self, haystack, start)
    }

    fn capture_names(&self) -> impl Iterator<Item = Option<&str>> {
        Regex::capture_names(self)
    }
}

impl Search for Alternation {
    fn captures_at<'h>(&self, haystack: &'h str, start: usize) -> Option<Captures<'h>> {
        Alternation::captures_at(self, haystack, start)
    }

    fn capture_names(&self) -> impl Iterator<Item = Option<&str>> {
        Alternation::capture_names(self)
    }
}
