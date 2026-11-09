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
        Searcher::new(&self.0, haystack).captures_at(start)
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
        let mut searcher = Searcher::new(&self.0, haystack);
        let mut at = 0;
        let mut last_end = None;
        Box::new(std::iter::from_fn(move || {
            while at <= haystack.len() {
                let found = searcher.captures_at(at)?;
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

    /// Every group name of the regexes, in their order, a name two of them
    /// share once: a match's captures hold the groups of the regex that
    /// found it, and a group of the others is none in them.
    pub(super) fn capture_names(&self) -> impl Iterator<Item = Option<&str>> {
        let mut seen = std::collections::HashSet::new();
        self.0
            .iter()
            .flat_map(Regex::capture_names)
            .filter(move |name| name.is_none_or(|name| seen.insert(name)))
    }
}

/// Searches one haystack, again and again, for the first match of any of
/// `forms` at or after a start, as their alternation would find it.
///
/// It remembers each form's last search: where it started and the match
/// it found, or that it found none. That match is the form's first one
/// from any start between the two, and none stays none from any later
/// start, so a form is searched again only once a start passes its match
/// or goes back before its search. A pattern's matches, each searched for
/// after the last, then cost each form one pass over the haystack, not
/// one pass per match. The form whose match is returned is searched
/// again, from the next start, as one regex would be.
pub(super) struct Searcher<'r, 'h> {
    forms: &'r [Regex],
    haystack: &'h str,
    /// Per form: where its last search started, and what it found.
    last: Vec<Option<(usize, Option<Captures<'h>>)>>,
}

impl<'r, 'h> Searcher<'r, 'h> {
    pub(super) fn new(forms: &'r [Regex], haystack: &'h str) -> Self {
        Searcher {
            forms,
            haystack,
            last: std::iter::repeat_with(|| None).take(forms.len()).collect(),
        }
    }

    /// The first match at or after `start`, as one regex of the forms'
    /// alternation would find it.
    pub(super) fn captures_at(&mut self, start: usize) -> Option<Captures<'h>> {
        if let [regex] = self.forms {
            let found = regex.captures_at(self.haystack, start);
            scanned(self.haystack, start, found.as_ref());
            return found;
        }
        let mut first: Option<(usize, usize)> = None;
        for (index, (regex, last)) in self.forms.iter().zip(&mut self.last).enumerate() {
            let still = last.as_ref().is_some_and(|(from, found)| {
                *from <= start
                    && found
                        .as_ref()
                        .is_none_or(|found| start <= match_start(found))
            });
            if !still {
                let found = regex.captures_at(self.haystack, start);
                scanned(self.haystack, start, found.as_ref());
                *last = Some((start, found));
            }
            if let Some((_, Some(found))) = last
                && first.is_none_or(|(_, first)| match_start(found) < first)
            {
                first = Some((index, match_start(found)));
            }
        }
        // The match goes to the caller (captures are no `Clone`): its form
        // is searched again next time.
        let (index, _) = first?;
        self.last[index].take().and_then(|(_, found)| found)
    }
}

/// Where a match starts.
fn match_start(found: &Captures<'_>) -> usize {
    found.get(0).expect("group 0 is the match").start()
}

#[cfg(test)]
thread_local! {
    /// The bytes this thread's searches covered, each from its start to
    /// its match's end or the haystack's: the work a test counts.
    pub(super) static SCANNED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts a search's bytes in [`SCANNED`] in tests.
#[cfg(test)]
fn scanned(haystack: &str, start: usize, found: Option<&Captures<'_>>) {
    let end = found.map_or(haystack.len(), |found| {
        found.get(0).expect("group 0 is the match").end()
    });
    SCANNED.with(|scanned| scanned.set(scanned.get() + end.saturating_sub(start)));
}

#[cfg(not(test))]
fn scanned(_: &str, _: usize, _: Option<&Captures<'_>>) {}

/// What [`super::spans::span_ranges`] searches with: a regex, or an
/// alternation of them.
pub(super) trait Search {
    /// A searcher of `haystack`.
    fn searcher<'r, 'h>(&'r self, haystack: &'h str) -> Searcher<'r, 'h>;
    fn capture_names(&self) -> impl Iterator<Item = Option<&str>>;
}

impl Search for Regex {
    fn searcher<'r, 'h>(&'r self, haystack: &'h str) -> Searcher<'r, 'h> {
        Searcher::new(std::slice::from_ref(self), haystack)
    }

    fn capture_names(&self) -> impl Iterator<Item = Option<&str>> {
        Regex::capture_names(self)
    }
}

impl Search for Alternation {
    fn searcher<'r, 'h>(&'r self, haystack: &'h str) -> Searcher<'r, 'h> {
        Searcher::new(&self.0, haystack)
    }

    fn capture_names(&self) -> impl Iterator<Item = Option<&str>> {
        Alternation::capture_names(self)
    }
}
