//! Case coordination (ADR 0049, ADR 0041): the saved cases and the key
//! that opens them, one background write at a time with the latest
//! revision of each case waiting, and a quit that waits for those writes.
//! Generic over the window handle, so its rules run without a window; the
//! workspace view reads the key, runs the writes, shows prompts and
//! carries out the returned effects.

use mdoc_store::cases::{Case, CaseSummary, Store};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// What the workspace view carries out for a case transition.
#[derive(Debug, PartialEq)]
pub(super) enum CaseEffect<W> {
    /// Save this revision with the store in the background, then report
    /// with `written`.
    Write(Case),
    /// The tab's new case is saved: close the tab now.
    CloseTab(u64),
    /// A write failed; tell the user.
    WriteFailed(String),
    /// A quit waits on case saves in this window: try to finish it.
    ResumeQuit(W),
}

/// What the workspace view carries out to read the case list. Kept apart
/// from `CaseEffect` because it needs the window, which write completions
/// arriving from the background do not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaseLoad {
    /// Members left locked can load now: start loading the list.
    Load,
    /// Read every case with the store in the background, then
    /// `finish_load`.
    List,
}

/// Why case members wait for the key (ADR 0043).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyWait {
    /// The macOS Keychain dialog was declined.
    Denied,
    /// The key was not read at launch because reading it may ask.
    Locked,
}

/// The quit's final case saves (ADR 0049).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Flush {
    /// A failure prompt is open; wait for its answer.
    Prompting,
    /// A write failed: ask Retry, Cancel or Quit without saving mappings.
    Ask(String),
    /// Collect every member's latest mapping, then `flushed`.
    Collect,
}

pub(super) struct CaseCoordinator<W> {
    store: Option<Arc<Store>>,
    /// Every readable case, once loaded.
    cases: Vec<Case>,
    summaries: Vec<CaseSummary>,
    loaded: bool,
    loading: bool,
    /// Latest unsaved state per case; one write runs at a time.
    pending: HashMap<String, Case>,
    /// The case being written now.
    writing: Option<Case>,
    /// Cases whose last write failed; kept pending for the next attempt.
    failed: HashSet<String>,
    /// Tabs to close once their new case is safely written.
    close_after: HashMap<String, u64>,
    /// Tabs whose close already asked about their unsaved mapping.
    asked: HashSet<u64>,
    /// The window waiting for its last case revisions before closing.
    quit_window: Option<W>,
    quit_error: Option<String>,
    quit_prompting: bool,
    /// Case members wait for an explicit unlock: the key was not read at
    /// launch because reading it may ask for the Mac password.
    locked: bool,
    /// How many case files the folder held when members were left locked,
    /// counted by name without the key.
    locked_files: Option<usize>,
    /// The last macOS Keychain dialog was declined.
    denied: bool,
}

impl<W> Default for CaseCoordinator<W> {
    fn default() -> Self {
        Self {
            store: None,
            cases: Vec::new(),
            summaries: Vec::new(),
            loaded: false,
            loading: false,
            pending: HashMap::new(),
            writing: None,
            failed: HashSet::new(),
            close_after: HashMap::new(),
            asked: HashSet::new(),
            quit_window: None,
            quit_error: None,
            quit_prompting: false,
            locked: false,
            locked_files: None,
            denied: false,
        }
    }
}

impl<W: Clone> CaseCoordinator<W> {
    // ---- The key ----------------------------------------------------------

    pub(super) fn store(&self) -> Option<&Arc<Store>> {
        self.store.as_ref()
    }

    /// The key opened.
    pub(super) fn open(&mut self, store: Arc<Store>) {
        if self.store.is_none() {
            self.loaded = false;
        }
        self.store = Some(store);
    }

    /// Use `store` and read every case again.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(super) fn reopen(&mut self, store: Option<Arc<Store>>) {
        self.store = store;
        self.loaded = false;
    }

    /// Why members wait for the key, if they do.
    pub(super) fn key_wait(&self) -> Option<KeyWait> {
        if self.store.is_some() {
            None
        } else if self.denied {
            Some(KeyWait::Denied)
        } else if self.locked {
            Some(KeyWait::Locked)
        } else {
            None
        }
    }

    #[cfg(test)]
    pub(super) fn locked(&self) -> bool {
        self.locked
    }

    pub(super) fn denied(&self) -> bool {
        self.denied
    }

    pub(super) fn deny(&mut self, denied: bool) {
        self.denied = denied;
    }

    /// Launch left members locked: reading the key may ask.
    pub(super) fn lock(&mut self) {
        self.locked = true;
    }

    /// The number of case files counted while members wait for the key.
    pub(super) fn count_locked(&mut self, files: Option<usize>) {
        self.locked_files = files;
    }

    /// How many saved cases wait for the key, when that is known: the files
    /// counted in the cases folder, else the cases the session remembers.
    pub(super) fn locked_count(&self) -> Option<usize> {
        self.key_wait()?;
        self.locked_files
            .or_else(|| Some(self.summaries.len()).filter(|&n| n > 0))
    }

    /// The key is open: members left locked at launch load now.
    pub(super) fn unlock(&mut self) -> Option<CaseLoad> {
        if self.store.is_some() && std::mem::take(&mut self.locked) {
            self.loaded = false;
            Some(CaseLoad::Load)
        } else {
            None
        }
    }

    // ---- The case list ----------------------------------------------------

    pub(super) fn cases(&self) -> &[Case] {
        &self.cases
    }

    pub(super) fn restore_summaries(&mut self, summaries: Vec<CaseSummary>) {
        self.summaries = summaries;
    }

    pub(super) fn summaries(&self) -> Vec<CaseSummary> {
        let mut summaries = self.summaries.clone();
        for case in &self.cases {
            summaries.retain(|summary| summary.id != case.id);
            summaries.push(CaseSummary::of(case));
        }
        summaries
    }

    /// The list as is, for tests that set up cases without saving them.
    #[cfg(test)]
    pub(super) fn cases_mut(&mut self) -> &mut Vec<Case> {
        &mut self.cases
    }

    pub(super) fn case(&self, id: &str) -> Option<&Case> {
        self.cases.iter().find(|case| case.id == id)
    }

    pub(super) fn loaded(&self) -> bool {
        self.loaded
    }

    /// Whether a load may start: not loaded, not loading, not locked.
    pub(super) fn may_load(&self) -> bool {
        !self.loaded && !self.loading && !self.locked
    }

    /// There is nothing to read: no cases folder.
    pub(super) fn mark_loaded(&mut self) {
        self.loaded = true;
    }

    pub(super) fn begin_load(&mut self) -> CaseLoad {
        self.loading = true;
        CaseLoad::List
    }

    /// The cases were read; those created while loading are kept.
    pub(super) fn finish_load(&mut self, cases: Vec<Case>) {
        self.loading = false;
        self.loaded = true;
        for case in cases {
            if !self.cases.iter().any(|c| c.id == case.id) {
                self.cases.push(case);
            }
        }
        self.summaries = self.cases.iter().map(CaseSummary::of).collect();
    }

    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn hide_for_locked_render(&mut self) {
        self.cases.clear();
    }

    /// A new case, first in the list, saved now.
    pub(super) fn create(&mut self, case: Case) -> Option<CaseEffect<W>> {
        self.cases.insert(0, case.clone());
        self.enqueue(case)
    }

    /// Change case `id`. `edit` returns what the caller needs when it
    /// changed the case, which is then saved, and none when it did not.
    pub(super) fn edit<R>(
        &mut self,
        id: &str,
        edit: impl FnOnce(&mut Case) -> Option<R>,
    ) -> Option<(R, Option<CaseEffect<W>>)> {
        let case = self.cases.iter_mut().find(|case| case.id == id)?;
        let result = edit(case)?;
        let case = case.clone();
        Some((result, self.enqueue(case)))
    }

    /// Case `id` is deleted: its waiting revision goes, a write already
    /// running ends, and its failure is then dropped (`written`).
    pub(super) fn delete(&mut self, id: &str) {
        self.summaries.retain(|summary| summary.id != id);
        self.pending.remove(id);
        self.cases.retain(|case| case.id != id);
    }

    // ---- Writes -----------------------------------------------------------

    /// Save `case` as the next revision of the stored one; it replaces any
    /// revision still waiting.
    fn enqueue(&mut self, mut case: Case) -> Option<CaseEffect<W>> {
        if let Some(stored) = self.cases.iter_mut().find(|c| c.id == case.id) {
            stored.revision += 1;
            case.revision = stored.revision;
        }
        self.pending.insert(case.id.clone(), case);
        self.pump()
    }

    /// The next waiting case to write, unless a write runs or a failed
    /// quit is waiting for the user's answer.
    pub(super) fn pump(&mut self) -> Option<CaseEffect<W>> {
        if self.writing.is_some() || self.quit_error.is_some() {
            return None;
        }
        let id = self.pending.keys().next().cloned()?;
        let case = self.pending.remove(&id)?;
        self.writing = Some(case.clone());
        Some(CaseEffect::Write(case))
    }

    /// Whether a write runs or waits.
    pub(super) fn saving(&self) -> bool {
        self.writing.is_some() || !self.pending.is_empty()
    }

    /// The running write ended with `result`; `finishing` says whether a
    /// quit is in its final saves. None when no write was running.
    pub(super) fn written(
        &mut self,
        result: Result<(), String>,
        finishing: bool,
    ) -> Option<Vec<CaseEffect<W>>> {
        let case = self.writing.take()?;
        let mut effects = Vec::new();
        match result {
            Ok(()) => {
                self.failed.remove(&case.id);
                if !self.pending.contains_key(&case.id)
                    && let Some(tab) = self.close_after.remove(&case.id)
                {
                    effects.push(CaseEffect::CloseTab(tab));
                }
                effects.extend(self.pump());
            }
            Err(error) => effects.extend(self.write_failed(case, error, finishing)),
        }
        effects.extend(self.quit_window.clone().map(CaseEffect::ResumeQuit));
        Some(effects)
    }

    fn write_failed(&mut self, case: Case, error: String, finishing: bool) -> Vec<CaseEffect<W>> {
        if !self.cases.iter().any(|current| current.id == case.id) {
            // Removed while writing: nothing to retry, but release its close
            // and keep the rest of the queue moving.
            self.failed.remove(&case.id);
            if let Some(tab) = self.close_after.remove(&case.id) {
                self.asked.remove(&tab);
            }
            return self.pump().into_iter().collect();
        }
        self.failed.insert(case.id.clone());
        if let Some(tab) = self.close_after.remove(&case.id) {
            self.asked.remove(&tab);
        }
        // A later revision already queued must win over the failed one.
        self.pending.entry(case.id.clone()).or_insert(case);
        // Only a quit still in progress owns the failure; an abandoned one
        // must not block ordinary saves behind an unanswered prompt.
        if finishing && self.quit_window.is_some() {
            self.quit_error = Some(error.clone());
        }
        vec![CaseEffect::WriteFailed(error)]
    }

    /// Whether the last write of case `id` failed.
    pub(super) fn failed(&self, id: &str) -> bool {
        self.failed.contains(id)
    }

    /// Close tab `tab` once case `id` is saved.
    pub(super) fn close_after(&mut self, id: String, tab: u64) {
        self.close_after.insert(id, tab);
    }

    /// Save as Case for tab `tab` of existing case `id` (#189): close it once
    /// the waiting revision (a failed one, retried now) is written, or at once
    /// when nothing waits, e.g. it saved while the close prompt was open.
    pub(super) fn save_and_close(&mut self, id: String, tab: u64) -> Vec<CaseEffect<W>> {
        let waiting = self.pending.contains_key(&id)
            || self.writing.as_ref().is_some_and(|case| case.id == id);
        if !waiting {
            return vec![CaseEffect::CloseTab(tab)];
        }
        self.close_after(id, tab);
        self.note_asked(tab);
        self.pump().into_iter().collect()
    }

    // ---- Closing tabs with a mapping ---------------------------------------

    /// Whether closing tab `id` already asked about its unsaved mapping.
    pub(super) fn asked(&self, id: u64) -> bool {
        self.asked.contains(&id)
    }

    pub(super) fn note_asked(&mut self, id: u64) {
        self.asked.insert(id);
    }

    pub(super) fn forget_asked(&mut self, id: u64) {
        self.asked.remove(&id);
    }

    // ---- Quitting -----------------------------------------------------------

    /// The quit's final case saves, in `window`.
    pub(super) fn flush(&mut self, window: W) -> Flush {
        if self.quit_prompting {
            return Flush::Prompting;
        }
        self.quit_window = Some(window);
        match self.quit_error.clone() {
            Some(error) => {
                self.quit_prompting = true;
                Flush::Ask(error)
            }
            None => Flush::Collect,
        }
    }

    /// Every member's mapping is queued: whether all are saved, so the
    /// window may close.
    pub(super) fn flushed(&mut self) -> bool {
        if self.saving() {
            return false;
        }
        self.quit_window = None;
        true
    }

    /// The failure prompt was answered; the caller retries, cancels or
    /// quits.
    pub(super) fn answered(&mut self) {
        self.quit_prompting = false;
        self.quit_error = None;
    }

    /// Quit without the latest mappings: nothing waits any more.
    pub(super) fn give_up(&mut self) {
        self.quit_window = None;
    }

    /// Every path that abandons a quit clears all case-quit state (ADR 0049).
    pub(super) fn cancel_quit(&mut self) {
        self.quit_window = None;
        self.quit_error = None;
        self.quit_prompting = false;
    }

    /// The window a quit waits in, if any.
    pub(super) fn quit_window(&self) -> Option<&W> {
        self.quit_window.as_ref()
    }

    #[cfg(test)]
    pub(super) fn set_quit_window(&mut self, window: Option<W>) {
        self.quit_window = window;
    }

    #[cfg(test)]
    pub(super) fn quit_pending(&self) -> bool {
        self.quit_window.is_some() || self.quit_error.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(name: &str) -> Case {
        Case::new(name.into(), &[], None)
    }

    fn written_case(effect: Option<CaseEffect<u8>>) -> Case {
        match effect {
            Some(CaseEffect::Write(case)) => case,
            other => panic!("expected a write, got {other:?}"),
        }
    }

    #[test]
    fn one_write_runs_and_the_latest_revision_waits() {
        let mut cases = CaseCoordinator::<u8>::default();
        let first = written_case(cases.create(case("Smith v. Jones")));
        let id = first.id.clone();
        assert!(cases.saving());
        // Two edits while the first write runs: only the latest waits.
        let (_, effect) = cases
            .edit(&id, |case| {
                case.name = "Smith".into();
                Some(())
            })
            .unwrap();
        assert_eq!(effect, None);
        let (_, effect) = cases
            .edit(&id, |case| {
                case.name = "Smith & Co".into();
                Some(())
            })
            .unwrap();
        assert_eq!(effect, None);
        // An edit that changes nothing saves nothing.
        assert!(cases.edit(&id, |_| None::<()>).is_none());
        let effects = cases.written(Ok(()), false).unwrap();
        let [CaseEffect::Write(next)] = effects.as_slice() else {
            panic!("the waiting revision is written next: {effects:?}");
        };
        assert_eq!(next.name, "Smith & Co");
        assert_eq!(next.revision, cases.case(&id).unwrap().revision);
        assert!(cases.written(Ok(()), false).unwrap().is_empty());
        assert!(!cases.saving());
        assert_eq!(cases.written(Ok(()), false), None, "no write runs");
    }

    #[test]
    fn a_failed_write_stays_pending_and_a_quit_owns_the_failure() {
        let mut cases = CaseCoordinator::<u8>::default();
        let id = written_case(cases.create(case("Acme"))).id;
        cases.close_after(id.clone(), 4);
        cases.note_asked(4);
        assert_eq!(cases.flush(1), Flush::Collect);
        let effects = cases.written(Err("disk full".into()), true).unwrap();
        assert_eq!(
            effects,
            [
                CaseEffect::WriteFailed("disk full".into()),
                CaseEffect::ResumeQuit(1)
            ]
        );
        assert!(cases.failed(&id) && cases.saving() && !cases.asked(4));
        // The failure blocks writes until the user answers.
        assert_eq!(cases.pump(), None);
        assert_eq!(cases.flush(1), Flush::Ask("disk full".into()));
        assert_eq!(cases.flush(1), Flush::Prompting);
        cases.answered();
        let retry = written_case(cases.pump());
        assert_eq!(retry.id, id);
        assert_eq!(
            cases.written(Ok(()), true).unwrap(),
            [CaseEffect::ResumeQuit(1)]
        );
        assert!(!cases.failed(&id));
        assert!(cases.flushed());
        assert_eq!(cases.quit_window(), None);
    }

    #[test]
    fn retrying_a_failed_case_closes_its_tab_once_written() {
        let mut cases = CaseCoordinator::<u8>::default();
        let id = written_case(cases.create(case("Acme"))).id;
        cases.written(Err("disk full".into()), false).unwrap();
        assert!(cases.failed(&id));
        let retry = written_case(cases.save_and_close(id.clone(), 4).pop());
        assert_eq!(retry.id, id);
        assert!(cases.asked(4));
        assert_eq!(
            cases.written(Ok(()), false).unwrap(),
            [CaseEffect::CloseTab(4)]
        );
        assert!(!cases.failed(&id));
        // Saved meanwhile: nothing waits, so the tab closes at once.
        assert_eq!(cases.save_and_close(id, 5), [CaseEffect::CloseTab(5)]);
        assert!(!cases.asked(5));
    }

    #[test]
    fn a_saved_case_closes_its_tab_unless_a_newer_revision_waits() {
        let mut cases = CaseCoordinator::<u8>::default();
        let id = written_case(cases.create(case("Acme"))).id;
        cases.close_after(id.clone(), 4);
        cases.edit(&id, |case| {
            case.name = "Acme Ltd".into();
            Some(())
        });
        let effects = cases.written(Ok(()), false).unwrap();
        assert!(matches!(effects.as_slice(), [CaseEffect::Write(_)]));
        assert_eq!(
            cases.written(Ok(()), false).unwrap(),
            [CaseEffect::CloseTab(4)]
        );
    }

    #[test]
    fn a_case_removed_while_writing_drops_its_failure_and_keeps_the_queue_moving() {
        let mut cases = CaseCoordinator::<u8>::default();
        let gone = written_case(cases.create(case("Gone"))).id;
        let kept = case("Kept");
        let kept_id = kept.id.clone();
        assert_eq!(cases.create(kept), None);
        cases.close_after(gone.clone(), 2);
        cases.note_asked(2);
        cases.delete(&gone);
        assert!(cases.case(&gone).is_none());
        let effects = cases.written(Err("gone".into()), true).unwrap();
        let [CaseEffect::Write(next)] = effects.as_slice() else {
            panic!("the next case is written: {effects:?}");
        };
        assert_eq!(next.id, kept_id);
        assert!(!cases.failed(&gone) && !cases.asked(2));
    }

    #[test]
    fn the_key_and_loading_follow_launch_and_unlock() {
        let mut cases = CaseCoordinator::<u8>::default();
        assert!(cases.may_load());
        cases.lock();
        assert_eq!(cases.key_wait(), Some(KeyWait::Locked));
        assert!(!cases.may_load());
        cases.deny(true);
        assert_eq!(cases.key_wait(), Some(KeyWait::Denied));
        // Without the key, unlocking does nothing.
        assert_eq!(cases.unlock(), None);
        cases.open(Arc::new(Store::with_key(
            "/nonexistent/cases".into(),
            &[3; 32],
        )));
        cases.deny(false);
        assert_eq!(cases.key_wait(), None);
        assert_eq!(cases.unlock(), Some(CaseLoad::Load));
        assert_eq!(cases.unlock(), None);
        assert!(cases.may_load());
        assert_eq!(cases.begin_load(), CaseLoad::List);
        assert!(!cases.may_load());
        let early = case("Created while loading");
        cases.create(early.clone());
        cases.finish_load(vec![early, case("Stored")]);
        assert!(cases.loaded());
        assert_eq!(cases.cases().len(), 2);
    }

    #[test]
    fn the_locked_count_prefers_the_counted_files_and_ends_with_the_key() {
        let mut cases = CaseCoordinator::<u8>::default();
        cases.count_locked(Some(4));
        // Not locked: nothing is waiting.
        assert_eq!(cases.locked_count(), None);
        cases.lock();
        assert_eq!(cases.locked_count(), Some(4));
        cases.count_locked(None);
        assert_eq!(cases.locked_count(), None);
        cases.restore_summaries(vec![CaseSummary::of(&case("Remembered"))]);
        assert_eq!(cases.locked_count(), Some(1));
        cases.open(Arc::new(Store::with_key(
            "/nonexistent/cases".into(),
            &[3; 32],
        )));
        assert_eq!(cases.locked_count(), None);
    }

    #[test]
    fn cancelling_a_quit_clears_every_case_quit_state() {
        let mut cases = CaseCoordinator::<u8>::default();
        cases.create(case("Acme"));
        cases.flush(1);
        cases.written(Err("disk full".into()), true).unwrap();
        assert!(cases.quit_pending());
        cases.cancel_quit();
        assert!(!cases.quit_pending());
        assert_eq!(cases.flush(1), Flush::Collect);
        cases.give_up();
        assert_eq!(cases.quit_window(), None);
    }
}
