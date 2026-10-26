//! Find-session ownership, scheduling, and editor integration.
//!
//! Replace, Replace all, Whole word and their keys follow the buffer search
//! bar of Zed's `search` crate (https://github.com/zed-industries/zed),
//! GPL-3.0-or-later; see THIRD-PARTY-LICENSES.html (ADR 0063).
use crate::document_view::DocumentView;
use gpui::{Context, Focusable, SharedString, Window, actions, px};
use mdoc_editor::{SearchIndex, SearchMatch, SearchOptions};
use std::{ops::Range, sync::Arc};
actions!(
    mdoc,
    [
        FindMarkdown,
        FindReplaceMarkdown,
        ReplaceNextMarkdown,
        ReplaceAllMarkdown,
        ToggleMarkdownWholeWord,
        FindNextMarkdown,
        FindPreviousMarkdown,
        CloseMarkdownSearch,
        ToggleMarkdownMatchCase,
    ]
);

#[derive(Clone, Copy)]
pub(super) enum SearchRefresh {
    Open,
    Query,
    DocumentEdit,
}

#[derive(Default)]
pub(super) struct SearchSession {
    open: bool,
    /// The Replace row is shown (⌘⌥F / Ctrl+H, ADR 0063).
    replace_open: bool,
    match_case: bool,
    whole_word: bool,
    /// The last Replace result, until the query or an option changes.
    status: Option<SharedString>,
    /// Where a Replace left off, in the edited text, so the next match is the
    /// one after the replacement rather than the replacement itself.
    next_anchor: Option<usize>,
    index: Option<SearchIndex>,
    source: Arc<str>,
    matches: Vec<SearchMatch>,
    active: Option<usize>,
    anchor: Option<usize>,
    revision: u64,
    task: Option<gpui::Task<()>>,
    pending: Option<SearchRequest>,
    running: bool,
}

struct SearchRequest {
    source: Arc<str>,
    query: String,
    options: SearchOptions,
    revision: u64,
    anchor: usize,
    should_scroll: bool,
}

// A running projection is never duplicated. New input replaces the single
// pending request. Completion may donate its index to a newer query, but only
// for the same immutable document snapshot.
impl SearchSession {
    /// The find bar is open.
    pub(crate) fn open(&self) -> bool {
        self.open
    }

    /// The Replace row is shown.
    pub(crate) fn replace_open(&self) -> bool {
        self.replace_open
    }

    pub(crate) fn match_case(&self) -> bool {
        self.match_case
    }

    pub(crate) fn whole_word(&self) -> bool {
        self.whole_word
    }

    /// The last Replace result, until the query or an option changes.
    pub(crate) fn status(&self) -> Option<&SharedString> {
        self.status.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn index(&self) -> Option<&SearchIndex> {
        self.index.as_ref()
    }

    /// The matches of the published query, in document order.
    pub(crate) fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }

    /// The highlighted match.
    pub(crate) fn active(&self) -> Option<usize> {
        self.active
    }

    /// A search is still being built or run.
    pub(crate) fn running(&self) -> bool {
        self.task.is_some()
    }

    /// Show the Replace row and a Replace result, for tests of how the bar
    /// lays out.
    #[cfg(test)]
    pub(crate) fn show_replace(&mut self, open: bool, status: Option<SharedString>) {
        self.replace_open = open;
        if status.is_some() {
            self.status = status;
        }
    }

    fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.pending = None;
        self.anchor = None;
        self.active = None;
        self.matches.clear();
    }

    fn take_work(&mut self) -> Option<(SearchRequest, Option<SearchIndex>)> {
        if self.running {
            return None;
        }
        let request = self.pending.take()?;
        self.running = true;
        Some((request, self.index.take()))
    }

    fn complete(&mut self, request: &SearchRequest, index: SearchIndex) -> bool {
        self.running = false;
        if Arc::ptr_eq(&self.source, &request.source) {
            self.index = Some(index);
        }
        self.open && self.revision == request.revision
    }

    fn options(&self) -> SearchOptions {
        SearchOptions {
            match_case: self.match_case,
            whole_word: self.whole_word,
        }
    }
}

/// Why Replace leaves a match alone (ADR 0063).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplaceSkip {
    /// Hidden Markdown syntax lies inside it.
    Formatting,
    /// It overlaps an applied alias.
    Alias,
}

/// `applied` holds the applied alias ranges in source order.
fn replace_skip(search_match: &SearchMatch, applied: &[Range<usize>]) -> Option<ReplaceSkip> {
    let [range] = search_match.source.as_slice() else {
        return Some(ReplaceSkip::Formatting);
    };
    if !search_match.replaceable {
        return Some(ReplaceSkip::Formatting);
    }
    let first = applied.partition_point(|alias| alias.end <= range.start);
    applied
        .get(first)
        .is_some_and(|alias| alias.start < range.end)
        .then_some(ReplaceSkip::Alias)
}

fn replace_all_status(replaced: usize, formatting: usize, alias: usize) -> String {
    let mut status = format!("Replaced {replaced}");
    let mut skipped = Vec::new();
    if formatting > 0 {
        skipped.push(format!("{formatting} (formatting)"));
    }
    if alias > 0 {
        skipped.push(format!("{alias} (alias)"));
    }
    if !skipped.is_empty() {
        status.push_str(" · skipped ");
        status.push_str(&skipped.join(" · "));
    }
    status
}

impl DocumentView {
    pub(super) fn find_markdown(
        &mut self,
        _: &FindMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_markdown_search(false, window, cx);
    }

    pub(super) fn find_replace_markdown(
        &mut self,
        _: &FindReplaceMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lifecycle.source_only() {
            // Nothing to replace in a source-only tab: the chord opens the
            // preview's find, as ⌘F does there (ADR 0063).
            if !self.preview.visible() {
                self.toggle_preview(window, cx);
            }
            self.original_selected = true;
            if let Some(pdf) = self.preview.pdf().cloned() {
                pdf.update(cx, |pdf, cx| {
                    if !pdf.search_is_open() {
                        pdf.toggle_search(window, cx);
                    }
                });
            }
            return;
        }
        self.open_markdown_search(true, window, cx);
    }

    /// Open find, with the Replace row or without it, focused on the query.
    pub(super) fn open_markdown_search(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pii.dismiss_popup();
        if self.lifecycle.source_only() {
            return;
        }
        // Search works in the editor (ADR 0042).
        if self.compare.is_some() {
            self.close_compare(window, cx);
        }
        // In a narrow window the Markdown pane must be the one shown.
        self.original_selected = false;
        self.search.replace_open = replace;
        self.search.open = true;
        self.refresh_markdown_search(SearchRefresh::Open, true, window, cx);
        self.markdown_search
            .update(cx, |input, cx| input.select_all(cx));
        window.focus(&self.markdown_search.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn find_next_markdown(
        &mut self,
        _: &FindNextMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The counter, not the last replace result, says where you are (ADR 0064).
        self.search.status = None;
        self.step_markdown_search(false, window, cx);
    }

    pub(super) fn find_previous_markdown(
        &mut self,
        _: &FindPreviousMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The counter, not the last replace result, says where you are (ADR 0064).
        self.search.status = None;
        self.step_markdown_search(true, window, cx);
    }

    pub(super) fn close_markdown_search(
        &mut self,
        _: &CloseMarkdownSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.search.open {
            return;
        }
        self.search.invalidate();
        self.search.open = false;
        self.editor.update(cx, |editor, cx| {
            editor.set_search_matches(Vec::new(), None, cx)
        });
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn toggle_markdown_match_case(
        &mut self,
        _: &ToggleMarkdownMatchCase,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.match_case = !self.search.match_case;
        if self.search.open {
            self.refresh_markdown_search(SearchRefresh::Query, true, window, cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn toggle_markdown_whole_word(
        &mut self,
        _: &ToggleMarkdownWholeWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.whole_word = !self.search.whole_word;
        if self.search.open {
            self.refresh_markdown_search(SearchRefresh::Query, true, window, cx);
        } else {
            cx.notify();
        }
    }

    /// Show or hide the Replace row from its toggle button.
    pub(super) fn toggle_markdown_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.replace_open = !self.search.replace_open;
        let focus = if self.search.replace_open {
            self.markdown_replace.read(cx).focus_handle(cx)
        } else {
            self.markdown_search.read(cx).focus_handle(cx)
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Make the published matches describe the current text. A refresh
    /// still pending (large documents search in the background) is done now,
    /// from where the last Replace left off, so Replace never acts on stale
    /// positions or the same occurrence twice (ADR 0063).
    fn settle_markdown_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text();
        if self.search.pending.is_none()
            && !self.search.running
            && text == self.search.source.as_ref()
        {
            return;
        }
        // The highlighted match while the text is unchanged (stepping moves
        // it past a skipped one), else where the last Replace left off.
        let unchanged = text == self.search.source.as_ref();
        let active = self
            .search
            .active
            .and_then(|index| self.search.matches.get(index))
            .and_then(search_match_start)
            .filter(|_| unchanged);
        let source: Arc<str> = Arc::from(text);
        let anchor = self
            .search
            .next_anchor
            .take()
            .or(active)
            .or(self.search.anchor)
            .unwrap_or_else(|| self.editor.read(cx).cursor());
        let query = self.markdown_search.read(cx).value().to_owned();
        // A running search finishes for the old revision and is discarded.
        self.search.invalidate();
        let index = SearchIndex::from_markdown(&source);
        let matches = if query.is_empty() {
            Vec::new()
        } else {
            index.find_with(&query, self.search.options())
        };
        self.search.source = source;
        self.search.index = Some(index);
        self.publish_markdown_search(matches, anchor, true, window, cx);
    }

    fn applied_alias_ranges(&self) -> Vec<Range<usize>> {
        self.pii
            .review()
            .applied()
            .iter()
            .map(|applied| applied.range.clone())
            .collect()
    }

    /// Replace the active match and move to the next one. A match Replace
    /// must leave alone is passed over with the reason in the status.
    pub(super) fn replace_next_markdown(
        &mut self,
        _: &ReplaceNextMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.search.open || !self.search.replace_open {
            return;
        }
        self.settle_markdown_search(window, cx);
        let Some(search_match) = self
            .search
            .active
            .and_then(|index| self.search.matches.get(index))
            .cloned()
        else {
            return;
        };
        match replace_skip(&search_match, &self.applied_alias_ranges()) {
            Some(skip) => {
                self.search.status = Some(
                    match skip {
                        ReplaceSkip::Formatting => "Skipped: crosses formatting",
                        ReplaceSkip::Alias => "Skipped: alias, change it in Replacements",
                    }
                    .into(),
                );
                self.step_markdown_search(false, window, cx);
            }
            None => {
                let range = search_match.source[0].clone();
                let replacement = self.markdown_replace.read(cx).value().to_owned();
                self.search.status = None;
                self.commit_markdown_replacements(
                    vec![(range.clone(), replacement.clone())],
                    range.start + replacement.len(),
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// Replace every replaceable match as one undo step (ADR 0063).
    pub(super) fn replace_all_markdown(
        &mut self,
        _: &ReplaceAllMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.search.open || !self.search.replace_open {
            return;
        }
        self.settle_markdown_search(window, cx);
        let matches = self.search.matches.clone();
        if matches.is_empty() {
            return;
        }
        let applied = self.applied_alias_ranges();
        let replacement = self.markdown_replace.read(cx).value().to_owned();
        let (mut formatting, mut alias) = (0, 0);
        let mut edits = Vec::new();
        for search_match in &matches {
            match replace_skip(search_match, &applied) {
                Some(ReplaceSkip::Formatting) => formatting += 1,
                Some(ReplaceSkip::Alias) => alias += 1,
                None => edits.push((search_match.source[0].clone(), replacement.clone())),
            }
        }
        edits.sort_by_key(|(range, _)| range.start);
        let replaced = edits.len();
        let cursor = self.editor.read(cx).cursor();
        let anchor = edits
            .iter()
            .filter(|(range, _)| range.end <= cursor)
            .fold(cursor, |at, (range, value)| at + value.len() - range.len());
        if !edits.is_empty() && !self.commit_markdown_replacements(edits, anchor, cx) {
            return;
        }
        self.search.status = Some(replace_all_status(replaced, formatting, alias).into());
        cx.notify();
    }

    /// One editor transaction; `anchor`, in the edited text, is where the
    /// next match is sought.
    fn commit_markdown_replacements(
        &mut self,
        edits: Vec<(Range<usize>, String)>,
        anchor: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let revision = self.editor.read(cx).revision();
        self.search.next_anchor = Some(anchor);
        let committed = self
            .editor
            .update(cx, |editor, cx| editor.replace_ranges(revision, &edits, cx));
        if !committed {
            self.search.next_anchor = None;
        }
        committed
    }

    pub(super) fn reset_markdown_search(&mut self, cx: &mut Context<Self>) {
        self.search.invalidate();
        self.search.open = false;
        self.search.replace_open = false;
        self.search.status = None;
        self.search.next_anchor = None;
        self.search.match_case = false;
        self.search.whole_word = false;
        self.search.index = None;
        self.search.source = Arc::from("");
        self.markdown_search.update(cx, |input, cx| input.reset(cx));
        self.markdown_replace
            .update(cx, |input, cx| input.reset(cx));
        self.editor.update(cx, |editor, cx| {
            editor.set_search_matches(Vec::new(), None, cx)
        });
    }

    pub(super) fn refresh_markdown_search(
        &mut self,
        reason: SearchRefresh,
        should_scroll: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Closed find must not copy the entire document on every keystroke.
        let text = self.editor.read(cx).text();
        if !self.search.open {
            if text != self.search.source.as_ref() {
                self.search.index = None;
                self.search.source = Arc::from("");
                self.search.invalidate();
            }
            return;
        }
        let source: Arc<str> = if text == self.search.source.as_ref() {
            self.search.source.clone()
        } else {
            Arc::from(text)
        };
        let old_anchor = self
            .search
            .active
            .and_then(|i| self.search.matches.get(i))
            .and_then(search_match_start)
            .or(self.search.anchor);
        let replaced_to = self.search.next_anchor.take();
        let should_scroll = should_scroll || replaced_to.is_some();
        // Any other edit (an undo, typing) makes the last result stale.
        if replaced_to.is_none() {
            self.search.status = None;
        }
        let anchor = match reason {
            SearchRefresh::DocumentEdit => replaced_to
                .or_else(|| {
                    old_anchor.map(|offset| map_edit_offset(&self.search.source, &source, offset))
                })
                .unwrap_or(self.editor.read(cx).cursor()),
            SearchRefresh::Open | SearchRefresh::Query => {
                self.search.status = None;
                self.editor.read(cx).cursor()
            }
        };
        if !Arc::ptr_eq(&source, &self.search.source) {
            self.search.index = None;
        }
        self.search.invalidate();
        self.search.source = source.clone();
        self.search.anchor = Some(anchor);
        let query = self.markdown_search.read(cx).value().to_owned();
        if query.is_empty() {
            self.publish_markdown_search(Vec::new(), anchor, false, window, cx);
            return;
        }
        self.search.pending = Some(SearchRequest {
            source,
            query,
            options: self.search.options(),
            revision: self.search.revision,
            anchor,
            should_scroll,
        });
        self.editor.update(cx, |editor, cx| {
            editor.set_search_matches(Vec::new(), None, cx)
        });
        self.start_search_work(window, cx);
        cx.notify();
    }

    fn start_search_work(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((request, index)) = self.search.take_work() else {
            return;
        };
        if request.source.len() > 64 * 1024 {
            let work = cx.background_executor().spawn(async move {
                let index = index.unwrap_or_else(|| SearchIndex::from_markdown(&request.source));
                let matches = index.find_with(&request.query, request.options);
                (request, index, matches)
            });
            self.search.task = Some(cx.spawn_in(window, async move |this, cx| {
                let (request, index, matches) = work.await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.search.task = None;
                    this.finish_search_work(request, index, matches, window, cx);
                });
            }));
        } else {
            let index = index.unwrap_or_else(|| SearchIndex::from_markdown(&request.source));
            let matches = index.find_with(&request.query, request.options);
            self.finish_search_work(request, index, matches, window, cx);
        }
    }

    fn finish_search_work(
        &mut self,
        request: SearchRequest,
        index: SearchIndex,
        matches: Vec<SearchMatch>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search.complete(&request, index) {
            self.publish_markdown_search(
                matches,
                request.anchor,
                request.should_scroll,
                window,
                cx,
            );
        }
        self.start_search_work(window, cx);
    }

    pub(super) fn publish_markdown_search(
        &mut self,
        matches: Vec<SearchMatch>,
        anchor: usize,
        should_scroll: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = if matches.is_empty() {
            None
        } else {
            first_search_match_at_or_after(&matches, anchor).or(Some(0))
        };
        self.search.anchor = active.and_then(|i| search_match_start(&matches[i]));
        self.search.matches = matches;
        self.search.active = active;
        let revision = self.search.revision;
        self.editor.update(cx, |editor, cx| {
            editor.set_search_matches(self.search.matches.clone(), self.search.active, cx)
        });
        if should_scroll && self.search.active.is_some() {
            self.schedule_markdown_search_scroll(revision, window, cx);
        }
        cx.notify();
    }

    pub(super) fn step_markdown_search(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.search.open || self.search.matches.is_empty() {
            return;
        }
        let len = self.search.matches.len();
        let current = self.search.active.unwrap_or_else(|| {
            first_search_match_at_or_after(&self.search.matches, self.editor.read(cx).cursor())
                .unwrap_or(0)
        });
        self.search.active = Some(if backwards {
            (current + len - 1) % len
        } else {
            (current + 1) % len
        });
        self.search.revision = self.search.revision.wrapping_add(1);
        let revision = self.search.revision;
        self.editor.update(cx, |editor, cx| {
            editor.set_active_search_match(self.search.active, cx)
        });
        self.schedule_markdown_search_scroll(revision, window, cx);
        cx.notify();
    }

    pub(super) fn schedule_markdown_search_scroll(
        &self,
        revision: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refine_markdown_search_scroll(revision, 2, window, cx);
    }

    pub(super) fn refine_markdown_search_scroll(
        &self,
        revision: u64,
        remaining: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |view, cx| {
                if !view.search.open || view.search.revision != revision {
                    return;
                }
                if let Some(index) = view.search.active {
                    view.scroll_to_markdown_search(index, cx);
                    // on_next_frame runs before paint: new query highlights or
                    // focus-dependent wrapping may only acquire bounds afterward.
                    if remaining > 0 {
                        view.refine_markdown_search_scroll(revision, remaining - 1, window, cx);
                        cx.notify();
                    }
                }
            });
        });
    }

    pub(super) fn scroll_to_markdown_search(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(match_bounds) = self.editor.read(cx).search_match_bounds(index) else {
            return;
        };
        let viewport = self.scroll.bounds();
        let margin = px(24.);
        let mut offset = self.scroll.offset();
        if match_bounds.top() < viewport.top() + margin {
            offset.y += viewport.top() + margin - match_bounds.top();
        } else if match_bounds.bottom() > viewport.bottom() - margin {
            offset.y -= match_bounds.bottom() - (viewport.bottom() - margin);
        }
        let max = self.scroll.max_offset();
        let min_y = -max.y;
        if offset.y > px(0.) {
            offset.y = px(0.);
        }
        if offset.y < min_y {
            offset.y = min_y;
        }
        if self.scroll.offset() != offset {
            self.scroll.set_offset(offset);
            cx.notify();
        }
    }
}

fn search_match_start(search_match: &SearchMatch) -> Option<usize> {
    search_match.source.first().map(|range| range.start)
}

fn first_search_match_at_or_after(matches: &[SearchMatch], offset: usize) -> Option<usize> {
    matches.iter().position(|search_match| {
        search_match_start(search_match).is_some_and(|start| start >= offset)
    })
}

/// Map an old source offset through the smallest changed middle region. This
/// keeps the active occurrence stable across ordinary typing while remaining
/// UTF-8 safe at the common-prefix/common-suffix boundaries.
pub(super) fn map_edit_offset(old: &str, new: &str, offset: usize) -> usize {
    let old_bytes = old.as_bytes();
    let new_bytes = new.as_bytes();
    let mut prefix = 0;
    while prefix < old_bytes.len()
        && prefix < new_bytes.len()
        && old_bytes[prefix] == new_bytes[prefix]
    {
        prefix += 1;
    }
    while prefix > 0 && (!old.is_char_boundary(prefix) || !new.is_char_boundary(prefix)) {
        prefix -= 1;
    }

    let mut suffix = 0;
    while suffix < old_bytes.len().saturating_sub(prefix)
        && suffix < new_bytes.len().saturating_sub(prefix)
        && old_bytes[old_bytes.len() - 1 - suffix] == new_bytes[new_bytes.len() - 1 - suffix]
    {
        suffix += 1;
    }
    while suffix > 0
        && (!old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix))
    {
        suffix -= 1;
    }

    let offset = offset.min(old.len());
    if offset < prefix {
        return offset.min(new.len());
    }
    let old_changed_end = old.len().saturating_sub(suffix);
    if offset >= old_changed_end {
        return new.len().saturating_sub(old.len().saturating_sub(offset));
    }
    prefix.min(new.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue(session: &mut SearchSession, query: &str) {
        session.invalidate();
        session.pending = Some(SearchRequest {
            source: session.source.clone(),
            query: query.into(),
            options: SearchOptions::default(),
            revision: session.revision,
            anchor: 0,
            should_scroll: true,
        });
    }

    #[test]
    fn rapid_queries_share_one_build_and_only_latest_query_runs_next() {
        let mut session = SearchSession {
            open: true,
            source: Arc::from("alpha beta"),
            ..Default::default()
        };
        queue(&mut session, "a");
        let (first, index) = session.take_work().unwrap();
        assert!(index.is_none());
        for query in ["al", "alp", "alph", "alpha", "beta"] {
            queue(&mut session, query);
            assert!(session.take_work().is_none(), "only one worker may run");
        }
        let index = SearchIndex::from_markdown(&first.source);
        assert!(!session.complete(&first, index), "old query cannot publish");
        let (latest, index) = session.take_work().unwrap();
        assert_eq!(latest.query, "beta");
        let index = index.expect("reuse completed projection rather than rebuilding");
        assert_eq!(index.find_with(&latest.query, latest.options).len(), 1);
        assert!(session.complete(&latest, index));
        assert!(session.take_work().is_none());
    }

    #[test]
    fn reset_rejects_old_projection_and_releases_its_snapshot() {
        let mut session = SearchSession {
            open: true,
            source: Arc::from("old document"),
            ..Default::default()
        };
        let old = Arc::downgrade(&session.source);
        queue(&mut session, "old");
        let (request, _) = session.take_work().unwrap();
        session.invalidate();
        session.source = Arc::from("new document");
        queue(&mut session, "new");
        assert!(!session.complete(&request, SearchIndex::from_markdown(&request.source)));
        drop(request);
        assert!(old.upgrade().is_none());
        let (request, index) = session.take_work().unwrap();
        assert!(index.is_none(), "new document needs a new projection");
        assert_eq!(request.query, "new");
    }

    #[test]
    fn closing_drops_pending_input_and_never_publishes_running_result() {
        let mut session = SearchSession {
            open: true,
            source: Arc::from("alpha"),
            ..Default::default()
        };
        queue(&mut session, "a");
        let (request, _) = session.take_work().unwrap();
        queue(&mut session, "alpha");
        session.invalidate();
        session.open = false;
        assert!(!session.complete(&request, SearchIndex::from_markdown(&request.source)));
        assert!(session.take_work().is_none());
        assert!(
            session.index.is_some(),
            "same-document reopen can reuse projection"
        );
    }
}
