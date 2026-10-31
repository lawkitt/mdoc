//! Immutable scan jobs and generation/revision/configuration validation.
use crate::{document_view::DocumentView, settings_ui};
use gpui::Context;
use mdoc_pii::Category;
use mdoc_store::settings;
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

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
    /// Categories this tab's scans skip: its case's list once it joined one,
    /// otherwise the Settings default (ADR 0041, ADR 0076).
    pub(crate) fn scan_skip(&self) -> HashSet<Category> {
        if self.case.link().is_some() {
            return self.case.skip().clone();
        }
        self.preferences
            .borrow()
            .current
            .as_ref()
            .map(|p| p.categories.skipped())
            .unwrap_or_default()
    }
    /// Settings › Detection stopped detecting `categories` (ADR 0076). A tab
    /// without a case drops their unapplied scan proposals; applied
    /// replacements and manual additions stay. Members follow their case.
    pub(crate) fn default_categories_unticked(
        &mut self,
        categories: &HashSet<Category>,
        cx: &mut Context<Self>,
    ) {
        if self.case.link().is_some() {
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
        // Never start a scan that cannot load its model: ask first (ADR 0026).
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
        self.pii.scan_started = std::time::Instant::now();
        self.pii.job = Some(ScanJob {
            cancel: cancel.clone(),
            revision,
            generation,
            config: config.clone(),
            progress: progress.clone(),
        });
        let task = cx.background_executor().spawn(async move {
            mdoc_detect::scan_config_with_progress(&source, &cancel, &config, &|p| {
                progress.set(p.done, p.windows)
            })
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
        // A case can switch categories off (ADR 0041 #31); scans skip them.
        // Without a case, Settings › Detection decides (ADR 0076).
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
