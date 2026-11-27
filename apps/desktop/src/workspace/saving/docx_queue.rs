//! DOCX conversions waiting for the one supervised worker: tabs
//! queue their jobs, one runs at a time, and cancelled jobs or jobs of
//! closed tabs are skipped. The workspace view hands the next job to its
//! document, which runs it and reports back.
use crate::workspace::Workspace;
use gpui::{Context, Window};

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A tab's request to convert `path`, valid for its `generation`.
#[derive(Clone, Debug)]
pub(in crate::workspace) struct DocxJob {
    pub(in crate::workspace) id: u64,
    pub(in crate::workspace) path: PathBuf,
    pub(in crate::workspace) generation: u64,
    pub(in crate::workspace) cancel: Arc<AtomicBool>,
}

/// What the workspace view carries out for a queue transition.
#[derive(Debug)]
pub(in crate::workspace) enum DocxEffect {
    /// Run this job in its tab's document.
    Run(DocxJob),
}

#[derive(Default)]
pub(in crate::workspace) struct DocxQueue {
    waiting: VecDeque<DocxJob>,
    /// The tab and generation of the job running now.
    running: Option<(u64, u64)>,
}

impl DocxQueue {
    pub(in crate::workspace) fn push(&mut self, job: DocxJob) {
        self.waiting.push_back(job);
    }

    /// The next job to run, unless one is running. Cancelled jobs and jobs
    /// whose tab has no document (`open` is false) are dropped.
    pub(in crate::workspace) fn next(&mut self, open: impl Fn(u64) -> bool) -> Option<DocxEffect> {
        if self.running.is_some() {
            return None;
        }
        while let Some(job) = self.waiting.pop_front() {
            if job.cancel.load(Ordering::Relaxed) || !open(job.id) {
                continue;
            }
            self.running = Some((job.id, job.generation));
            return Some(DocxEffect::Run(job));
        }
        None
    }

    /// Tab `id`'s job for `generation` ended; a stale report changes
    /// nothing.
    pub(in crate::workspace) fn finished(&mut self, id: u64, generation: u64) {
        if self.running == Some((id, generation)) {
            self.running = None;
        }
    }

    /// Tab `id` closed: its waiting jobs go. A running job ends by itself.
    pub(in crate::workspace) fn forget(&mut self, id: u64) {
        self.waiting.retain(|job| job.id != id);
    }

    /// Nothing waits and nothing runs.
    #[cfg(test)]
    pub(in crate::workspace) fn idle(&self) -> bool {
        self.waiting.is_empty() && self.running.is_none()
    }

    /// Pretend tab `id`'s job for `generation` is running.
    #[cfg(test)]
    pub(in crate::workspace) fn occupy(&mut self, id: u64, generation: u64) {
        self.running = Some((id, generation));
    }
}

impl Workspace {
    pub(in crate::workspace) fn pump_docx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = &self.tabs;
        let effect = self
            .docx_queue
            .next(|id| tabs.get(id).is_some_and(|tab| tab.view().is_some()));
        if let Some(DocxEffect::Run(job)) = effect
            && let Some(view) = self.tabs.get(job.id).and_then(|tab| tab.view().cloned())
        {
            view.update(cx, |view, cx| {
                view.run_docx(job.path, job.generation, job.cancel, window, cx)
            });
        }
    }

    pub(in crate::workspace) fn docx_finished(
        &mut self,
        id: u64,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.docx_queue.finished(id, generation);
        self.pump_docx(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: u64, generation: u64) -> DocxJob {
        DocxJob {
            id,
            path: format!("/contracts/{id}.docx").into(),
            generation,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn ran(effect: Option<DocxEffect>) -> Option<(u64, u64)> {
        effect.map(|DocxEffect::Run(job)| (job.id, job.generation))
    }

    #[test]
    fn one_job_runs_at_a_time_in_order() {
        let mut queue = DocxQueue::default();
        queue.push(job(1, 1));
        queue.push(job(2, 1));
        assert_eq!(ran(queue.next(|_| true)), Some((1, 1)));
        assert_eq!(ran(queue.next(|_| true)), None);
        // A stale report leaves the running job in place.
        queue.finished(1, 0);
        assert_eq!(ran(queue.next(|_| true)), None);
        queue.finished(1, 1);
        assert_eq!(ran(queue.next(|_| true)), Some((2, 1)));
        queue.finished(2, 1);
        assert!(queue.idle());
    }

    #[test]
    fn cancelled_jobs_and_closed_tabs_are_skipped() {
        let mut queue = DocxQueue::default();
        let cancelled = job(1, 1);
        cancelled.cancel.store(true, Ordering::Relaxed);
        queue.push(cancelled);
        queue.push(job(2, 1));
        queue.push(job(3, 1));
        queue.push(job(4, 1));
        queue.forget(3);
        assert_eq!(ran(queue.next(|id| id != 2)), Some((4, 1)));
        queue.finished(4, 1);
        assert!(queue.idle());
        queue.occupy(9, 1);
        assert!(!queue.idle());
    }
}
