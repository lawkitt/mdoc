//! Closing a tab with unsaved changes, and quitting (ADR 0003): each dirty
//! tab is asked in turn, then the session is saved and the window closes,
//! unless a document changed again while that save ran. The workspace view
//! asks the documents, saves the session, flushes cases and closes the
//! window; this owner decides what comes next.

use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};

/// Whether a close or quit waits on a document's answer, shared read-only
/// with the documents: an import finishing then waits too (ADR 0069, one
/// channel each way).
#[derive(Clone, Default)]
pub(crate) struct CloseWatch(Rc<Cell<bool>>);

impl CloseWatch {
    pub(crate) fn resolving(&self) -> bool {
        self.0.get()
    }
}

/// What the workspace view carries out for a quit-flow transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QuitEffect {
    /// Activate tab `id` and ask its document to resolve its changes: for a
    /// quit, or to close that one tab.
    Resolve(u64),
    /// Every tab is resolved: activate `restore` (the tab active when the
    /// quit began), note each document's history (`check`), save the
    /// session and, once no write runs, `finish`.
    Settle { restore: Option<u64> },
    /// The tab whose close was asked for is resolved: remove it.
    Remove(u64),
}

/// What comes after the final session save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum QuitEnd {
    /// Documents changed while saving: cancel the case flush and ask these
    /// tabs again, starting with the effect.
    Ask(QuitEffect),
    /// Nothing changed: flush cases, then close the window.
    Close,
}

#[derive(Default)]
pub(super) struct QuitFlow {
    /// Dirty tabs still to ask, while quitting.
    queue: Option<VecDeque<u64>>,
    /// The tab active when the quit began; it is active again at the end.
    restore: Option<u64>,
    /// The tab whose close is being resolved.
    closing: Option<u64>,
    /// Every tab resolved: the final save and the window close run.
    finishing: bool,
    /// The history each document had when its changes were resolved, by tab.
    checked: HashMap<u64, u64>,
    /// `resolving()`, as the documents read it.
    watch: CloseWatch,
}

impl QuitFlow {
    pub(super) fn quitting(&self) -> bool {
        self.queue.is_some()
    }

    pub(super) fn finishing(&self) -> bool {
        self.finishing
    }

    #[cfg(test)]
    pub(super) fn closing(&self) -> Option<u64> {
        self.closing
    }

    /// A close or quit is waiting on a document's answer.
    pub(super) fn resolving(&self) -> bool {
        self.queue.is_some() || self.closing.is_some()
    }

    /// `resolving()` for the documents, kept current by every transition.
    pub(super) fn watch(&self) -> CloseWatch {
        self.watch.clone()
    }

    fn publish(&self) {
        self.watch.0.set(self.resolving());
    }

    /// Whether a quit may begin now.
    pub(super) fn may_quit(&self) -> bool {
        self.queue.is_none() && !self.finishing
    }

    /// Whether a single tab may be closed now.
    pub(super) fn may_close(&self) -> bool {
        !self.resolving() && !self.finishing
    }

    /// Quit with tab `active` in front: ask the `dirty` tabs in turn.
    pub(super) fn quit(&mut self, active: u64, dirty: VecDeque<u64>) -> QuitEffect {
        self.closing = None;
        self.restore = Some(active);
        self.queue = Some(dirty);
        self.publish();
        self.step()
    }

    /// The next tab to ask, or the end of the questions; nothing unless
    /// quitting.
    pub(super) fn advance(&mut self) -> Option<QuitEffect> {
        self.queue.is_some().then(|| self.step())
    }

    fn step(&mut self) -> QuitEffect {
        match self.queue.as_mut().and_then(VecDeque::pop_front) {
            Some(id) => QuitEffect::Resolve(id),
            None => QuitEffect::Settle {
                restore: self.restore.take(),
            },
        }
    }

    /// The questions are over: remember each document's history, by tab,
    /// and start the final save.
    pub(super) fn settle(&mut self, histories: impl IntoIterator<Item = (u64, u64)>) {
        self.checked = histories.into_iter().collect();
        self.finishing = true;
    }

    /// Whether tab `id`'s document changed since its changes were resolved.
    pub(super) fn changed_since(&self, id: u64, history: u64) -> bool {
        self.checked.get(&id).is_none_or(|seen| *seen != history)
    }

    /// The final save ended; `changed` are the dirty tabs that changed since
    /// they were resolved.
    pub(super) fn end(&mut self, changed: VecDeque<u64>) -> QuitEnd {
        if changed.is_empty() {
            self.finishing = true;
            return QuitEnd::Close;
        }
        self.finishing = false;
        self.queue = Some(changed);
        self.publish();
        QuitEnd::Ask(self.step())
    }

    /// Tab `id` has unsaved changes: ask its document before closing it.
    pub(super) fn close(&mut self, id: u64) -> QuitEffect {
        self.closing = Some(id);
        self.publish();
        QuitEffect::Resolve(id)
    }

    /// A document resolved its changes.
    pub(super) fn resolved(&mut self, id: u64) -> Option<QuitEffect> {
        if self.queue.is_some() {
            self.advance()
        } else if self.closing == Some(id) {
            self.closing = None;
            self.publish();
            Some(QuitEffect::Remove(id))
        } else {
            None
        }
    }

    /// The user cancelled a close or the quit.
    pub(super) fn cancel(&mut self) {
        self.queue = None;
        self.restore = None;
        self.closing = None;
        self.publish();
    }

    /// The quit stops: a session save failed, or the user cancelled the
    /// final case saves. Returns whether it was finishing.
    pub(super) fn abandon(&mut self) -> bool {
        let finishing = std::mem::take(&mut self.finishing);
        self.queue = None;
        self.publish();
        finishing
    }
}

/// How many documents the Quit summary names before "and N more".
const SUMMARY_NAMES: usize = 5;

/// The Quit summary's detail (ADR 0071): the at-risk documents by name, in
/// sidebar order, one per line, up to five, then "and N more".
pub(super) fn summary_detail(names: &[String]) -> String {
    let mut lines: Vec<String> = names.iter().take(SUMMARY_NAMES).cloned().collect();
    if names.len() > SUMMARY_NAMES {
        lines.push(format!("and {} more", names.len() - SUMMARY_NAMES));
    }
    format!(
        "{}\n\nReview each one, or discard them all and quit. A mapping not saved as a \
         case can't be recovered, so AI replies to its document can't be restored.",
        lines.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_names_five_documents_then_counts_the_rest() {
        let names = |n: usize| (1..=n).map(|i| format!("doc {i}.md")).collect::<Vec<_>>();
        let lines = |n: usize| {
            let detail = summary_detail(&names(n));
            let list = detail.split("\n\n").next().unwrap().to_owned();
            list.lines().map(str::to_owned).collect::<Vec<_>>()
        };
        assert_eq!(lines(2), ["doc 1.md", "doc 2.md"]);
        assert_eq!(lines(5), names(5));
        let mut seven = names(5);
        seven.push("and 2 more".into());
        assert_eq!(lines(7), seven);
        assert!(summary_detail(&names(2)).ends_with("can't be restored."));
    }

    #[test]
    fn a_quit_asks_each_dirty_tab_then_settles_on_the_first_active_one() {
        let mut quit = QuitFlow::default();
        assert!(quit.may_quit() && quit.may_close());
        assert_eq!(quit.quit(5, [2, 3].into()), QuitEffect::Resolve(2));
        assert!(quit.quitting() && quit.resolving() && !quit.may_quit());
        assert!(!quit.may_close());
        assert_eq!(quit.resolved(2), Some(QuitEffect::Resolve(3)));
        assert_eq!(
            quit.resolved(3),
            Some(QuitEffect::Settle { restore: Some(5) })
        );
        quit.settle([(2, 10), (3, 20)]);
        assert!(quit.finishing() && !quit.may_quit());
        assert!(!quit.changed_since(2, 10));
        assert!(quit.changed_since(3, 21));
        assert!(quit.changed_since(4, 0), "an unseen tab counts as changed");
        assert_eq!(quit.end(VecDeque::new()), QuitEnd::Close);
        assert!(quit.finishing());
    }

    #[test]
    fn a_change_during_the_final_save_asks_again() {
        let mut quit = QuitFlow::default();
        assert_eq!(
            quit.quit(1, VecDeque::new()),
            QuitEffect::Settle { restore: Some(1) }
        );
        quit.settle([(1, 7)]);
        assert_eq!(quit.end([1].into()), QuitEnd::Ask(QuitEffect::Resolve(1)));
        assert!(!quit.finishing() && quit.quitting());
        // The tab active at the start was restored once already.
        assert_eq!(quit.resolved(1), Some(QuitEffect::Settle { restore: None }));
    }

    #[test]
    fn closing_one_tab_removes_it_once_resolved() {
        let mut quit = QuitFlow::default();
        let watch = quit.watch();
        assert!(!watch.resolving());
        assert_eq!(quit.close(4), QuitEffect::Resolve(4));
        assert_eq!(quit.closing(), Some(4));
        assert!(quit.resolving() && !quit.may_close() && quit.may_quit());
        assert!(watch.resolving());
        assert_eq!(quit.resolved(9), None);
        assert_eq!(quit.resolved(4), Some(QuitEffect::Remove(4)));
        assert!(quit.may_close() && !watch.resolving());
        // A quit replaces a pending close.
        quit.close(4);
        quit.quit(4, [4].into());
        assert_eq!(quit.closing(), None);
    }

    #[test]
    fn cancelling_or_a_failed_save_ends_the_quit() {
        let mut quit = QuitFlow::default();
        quit.quit(1, [1, 2].into());
        assert!(quit.watch().resolving());
        quit.cancel();
        assert!(quit.may_quit() && quit.may_close() && !quit.watch().resolving());
        assert_eq!(quit.advance(), None);
        quit.quit(1, VecDeque::new());
        quit.settle([]);
        assert!(quit.watch().resolving());
        assert!(quit.abandon());
        assert!(quit.may_quit() && !quit.watch().resolving());
        assert!(!quit.abandon());
    }
}
