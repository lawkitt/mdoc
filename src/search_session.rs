//! Find-session ownership, scheduling, and editor integration.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum SearchRefresh {
    Open,
    Query,
    DocumentEdit,
}

#[derive(Default)]
pub(super) struct SearchSession {
    pub open: bool,
    pub match_case: bool,
    pub index: Option<SearchIndex>,
    pub source: Arc<str>,
    pub matches: Vec<SearchMatch>,
    pub active: Option<usize>,
    pub anchor: Option<usize>,
    pub revision: u64,
    pub task: Option<gpui::Task<()>>,
    pending: Option<SearchRequest>,
    running: bool,
}

struct SearchRequest {
    source: Arc<str>,
    query: String,
    match_case: bool,
    revision: u64,
    anchor: usize,
    should_scroll: bool,
}

// A running projection is never duplicated. New input replaces the single
// pending request. Completion may donate its index to a newer query, but only
// for the same immutable document snapshot.
impl SearchSession {
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
}

impl Workspace {
    pub(super) fn find_markdown(
        &mut self,
        _: &FindMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.source_only {
            return;
        }
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
        self.step_markdown_search(false, window, cx);
    }

    pub(super) fn find_previous_markdown(
        &mut self,
        _: &FindPreviousMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn reset_markdown_search(&mut self, cx: &mut Context<Self>) {
        self.search.invalidate();
        self.search.open = false;
        self.search.match_case = false;
        self.search.index = None;
        self.search.source = Arc::from("");
        self.markdown_search.update(cx, |input, cx| input.reset(cx));
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
        let anchor = match reason {
            SearchRefresh::DocumentEdit => old_anchor
                .map(|offset| map_edit_offset(&self.search.source, &source, offset))
                .unwrap_or(self.editor.read(cx).cursor()),
            SearchRefresh::Open | SearchRefresh::Query => self.editor.read(cx).cursor(),
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
            match_case: self.search.match_case,
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
                let matches = index.find(&request.query, request.match_case);
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
            let matches = index.find(&request.query, request.match_case);
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
            let _ = weak.update(cx, |workspace, cx| {
                if !workspace.search.open || workspace.search.revision != revision {
                    return;
                }
                if let Some(index) = workspace.search.active {
                    workspace.scroll_to_markdown_search(index, cx);
                    // on_next_frame runs before paint: new query highlights or
                    // focus-dependent wrapping may only acquire bounds afterward.
                    if remaining > 0 {
                        workspace.refine_markdown_search_scroll(
                            revision,
                            remaining - 1,
                            window,
                            cx,
                        );
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
            match_case: false,
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
        assert_eq!(index.find(&latest.query, latest.match_case).len(), 1);
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
