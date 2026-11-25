//! Immutable scan jobs and generation/revision/configuration validation.
use crate::{document_view::DocumentView, settings_ui};
use gpui::Context;
use mdoc_pii::Category;
use mdoc_store::{
    lists::{ListChanges, Lists},
    settings,
};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[cfg(test)]
thread_local! {
    /// Tests: scans start without running the detector; the test ends
    /// each with [`DocumentView::finish_scan`].
    static HOLD_SCANS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A scan only proposes candidates; Apply edits the document explicitly.
pub(super) struct ScanJob {
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) revision: u64,
    pub(super) generation: u64,
    pub(super) config: settings::PiiConfig,
    pub(super) progress: Arc<ScanCount>,
}

/// The running scan's model windows, done and in all, for the toolbar's
/// count (#309). The scanning thread writes it; each frame reads it. One
/// word, so a frame never reads a count from one report and a total from
/// another. No windows yet: the scan has not split the source.
#[derive(Default)]
pub(super) struct ScanCount(AtomicU64);

impl ScanCount {
    pub(super) fn set(&self, done: usize, windows: usize) {
        let pack = |n: usize| u32::try_from(n).unwrap_or(u32::MAX) as u64;
        self.0
            .store(pack(done) << 32 | pack(windows), Ordering::Relaxed);
    }
    /// `(done, windows)`.
    pub(super) fn get(&self) -> (usize, usize) {
        let word = self.0.load(Ordering::Relaxed);
        ((word >> 32) as usize, (word & u64::from(u32::MAX)) as usize)
    }
}

impl DocumentView {
    /// Categories this tab's scans skip: its Project's list once it joined one,
    /// otherwise the Settings default.
    pub(crate) fn scan_skip(&self) -> HashSet<Category> {
        if self.project.link().is_some() {
            return self.project.skip().clone();
        }
        self.preferences
            .borrow()
            .current
            .as_ref()
            .map(|p| p.categories.skipped())
            .unwrap_or_default()
    }
    /// The lists this tab's scans use: its Project's over the
    /// user-wide ones of Settings › Detection, or those alone outside a
    /// Project.
    pub(crate) fn scan_lists(&self, cx: &gpui::App) -> Lists {
        let panel = self.model_panel.read(cx);
        Lists::effective(
            self.project.link().map(|_| self.project.lists()),
            panel.user_lists(),
        )
    }
    /// What this tab's decisions put on its Project's lists now:
    /// the wordings of identities it kept whole on Never, the wordings it
    /// marked by hand, in their categories, on Always. A wording both kept
    /// and marked is kept; one taken off as Not a name is on neither.
    fn listed(&self, cx: &gpui::App) -> Lists {
        let mut listed = Lists::default();
        let path = self.editor.read(cx).history_path();
        let not_names: HashSet<_> = self
            .pii
            .review
            .not_a_name_wordings()
            .into_iter()
            .map(|(_, wording)| wording)
            .collect();
        for (wording, category) in self.pii.decisions.marked(&path) {
            if !not_names.contains(&*wording) {
                listed.add_always(&wording, category);
            }
        }
        for wording in self.pii.review.never_wordings() {
            listed.add_never(&wording);
        }
        listed
    }
    /// What changed in this tab's contribution to its Project's lists since
    /// the last save into the Project; `None` outside a Project or when nothing
    /// did.
    pub(crate) fn take_list_changes(&mut self, cx: &gpui::App) -> Option<ListChanges> {
        self.project.link()?;
        let listed = self.listed(cx);
        self.project.contribute(listed)
    }
    /// Settings › Detection stopped detecting `categories`. A tab
    /// without a Project drops their unapplied scan proposals; applied
    /// replacements and manual additions stay. Members follow their Project.
    pub(crate) fn default_categories_unticked(
        &mut self,
        categories: &HashSet<Category>,
        cx: &mut Context<Self>,
    ) {
        if self.project.link().is_some() {
            return;
        }
        let withdrawn: usize = categories
            .iter()
            .map(|&category| self.pii.review.withdraw_detected(category))
            .sum();
        if withdrawn > 0 {
            self.review_changed(cx);
            cx.notify();
        }
    }
    /// It entered the AI project (#1164 Decision 5): scan it, as
    /// Pseudonymize does, unless it was scanned already, is scanning, has
    /// no text yet, or the model isn't set up (no setup card appears by
    /// itself). The scan only proposes. Whether it started.
    pub(crate) fn scan_on_entry(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.can_copy_markdown() || self.pii.scanning() || !self.pii.scans.is_empty() {
            return false;
        }
        let model = match self.preferences.borrow().snapshot() {
            Ok(preferences) => settings::Model::Pii(preferences.pseudonymization.model),
            Err(_) => return false,
        };
        if !mdoc_detect::SUPPORTED || self.model_panel.read(cx).needs_setup(model) {
            return false;
        }
        self.start_pii_scan(cx);
        self.pii.scanning()
    }

    /// Tests: from now on on this thread, scans start without running the
    /// detector (`hold`), or as they do in the app.
    #[cfg(test)]
    pub(crate) fn hold_scans(hold: bool) {
        HOLD_SCANS.set(hold);
    }

    /// Tests: the running scan ends with `result`, as the detector would
    /// end it.
    #[cfg(test)]
    pub(crate) fn finish_scan(
        &mut self,
        result: Result<Vec<mdoc_pii::Detection>, String>,
        cx: &mut Context<Self>,
    ) {
        let job = self.pii.job.as_ref().expect("a scan runs");
        let (generation, revision) = (job.generation, job.revision);
        let identity = self.session.generation;
        self.complete_pii_scan(generation, identity, revision, result, cx);
    }

    pub(crate) fn start_pii_scan(&mut self, cx: &mut Context<Self>) {
        if !self.can_copy_markdown() {
            return;
        }
        self.pii.cancel();
        self.pii.error = None;
        self.pii.dismiss_popup();
        self.pii.reviewing = true;
        self.pii.mapping.begin_review();
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let source = editor.text().to_owned();
        let config = match self.preferences.borrow().snapshot() {
            Ok(p) => p.pseudonymization,
            Err(e) => {
                self.pii.error = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        // Never start a scan that cannot load its model: ask first.
        if mdoc_detect::SUPPORTED
            && self
                .model_panel
                .read(cx)
                .needs_setup(settings::Model::Pii(config.model))
        {
            self.pii.setup = true;
            cx.notify();
            return;
        }
        self.pii.setup = false;
        let generation = self.pii.generation;
        let identity = self.session.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(ScanCount::default());
        let lists = self.scan_lists(cx);
        self.pii.scan_started = std::time::Instant::now();
        self.pii.job = Some(ScanJob {
            cancel: cancel.clone(),
            revision,
            generation,
            config: config.clone(),
            progress: progress.clone(),
        });
        // Tests that hold scans end them with their own detections.
        #[cfg(test)]
        if HOLD_SCANS.get() {
            cx.notify();
            return;
        }
        let task = cx.background_executor().spawn(async move {
            mdoc_detect::scan_config_with_progress(&source, &cancel, &config, &|p| {
                progress.set(p.done, p.windows)
            })
            .map(|detections| mdoc_detect::lists::apply(&source, detections, &lists))
            .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.complete_pii_scan(generation, identity, revision, result, cx)
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn complete_pii_scan(
        &mut self,
        generation: u64,
        identity: u64,
        revision: u64,
        result: Result<Vec<mdoc_pii::Detection>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = &self.pii.job else {
            return;
        };
        if job.generation != generation
            || job.revision != revision
            || self.session.generation != identity
            || self.editor.read(cx).revision() != revision
            || job.cancel.load(Ordering::Relaxed)
        {
            return;
        }
        let config = job.config.clone();
        self.pii.job = None;
        self.pii.mapping.show();
        let source = self.editor.read(cx).text().to_owned();
        // A Project can switch categories off; scans skip them.
        // Without a Project, Settings › Detection decides.
        let skip = self.scan_skip();
        let result = result.map(|mut detections| {
            detections.retain(|d| !skip.contains(&d.category));
            detections
        });
        match result.and_then(|detections| self.pii.review.ingest(&source, detections)) {
            Ok(()) => {
                self.pii.error = None;
                self.model_panel.update(cx, |panel, cx| {
                    let model = settings::Model::Pii(config.model);
                    panel.statuses[settings_ui::Panel::index(model)] = settings_ui::Status::Ready;
                    cx.notify();
                });
                self.pii.scans.push(config);
                self.review_changed(cx);
                self.fade_in_candidates(cx);
            }
            Err(error) => {
                self.pii.error = Some(error);
            }
        }
        // A selection made before or during the scan may now qualify for
        // Replace, even when the scan failed.
        self.sync_selection_action(cx);
        cx.notify();
    }
}
