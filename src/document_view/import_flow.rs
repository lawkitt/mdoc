//! Converting this tab's source into Markdown: the one import
//! job and its slot in the app-wide import permit, automatic conversion of
//! opened PDFs and DOCX files, OCR consent, recognition languages and
//! setup on the conversion card, the password card of a locked document,
//! and the tab's unlock of it. The document view reads Settings, starts the
//! background work and the model setup, and carries out the returned
//! effects.

use mdoc_import::{
    import::{ImportError, Imported, Password, Unlock},
    import_session::{ImportCompletion, ImportPermit, ImportSession},
    ocr, recognition,
};
use mdoc_store::settings::OcrModel;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// The card asks for a locked document's password: first, or again after
/// a wrong one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasswordPrompt {
    Asking,
    Wrong,
}

/// What the document view carries out for an import transition.
pub(crate) enum ImportEffect {
    /// Convert `path`: `skip_ocr` converts without recognition, `allow_ocr`
    /// recognizes text with this document's model when it is installed.
    Convert {
        path: PathBuf,
        skip_ocr: bool,
        allow_ocr: bool,
    },
    /// Setup finished: convert `path` with the installation it checked.
    ConvertWith {
        path: PathBuf,
        installed: ocr::Installed,
    },
    /// Run the conversion in the background, holding `permit` and stopping
    /// when `cancel` is set; report with `complete`, handing back what
    /// `unlock` became. `unlock` is the tab's, or a password typed on the
    /// card.
    Run {
        path: PathBuf,
        unlock: Option<Unlock>,
        installed: Option<ocr::Installed>,
        skip_ocr: bool,
        permit: Arc<ImportPermit>,
        cancel: Arc<AtomicBool>,
    },
    /// Set up this document's OCR model, then `finish_setup`.
    SetUp,
    /// The conversion is ready: replace the document with it.
    Request(Imported),
    /// The Settings model is installed: every tab shares that.
    SharedOcrReady(ocr::Installed),
    /// The Settings model failed: every tab shares that.
    SharedOcrFailed(String),
    /// Hand the shared state back to what Settings knows: this tab claimed
    /// it for a model that is no longer the Settings one.
    SharedOcrKnown,
    /// Tell every tab the shared OCR state.
    SharedOcrChanged,
    /// Show this error on the document.
    Error(String),
    /// The card asks for OCR consent: its primary action takes focus once.
    FocusCard,
    /// The import's state changed; the window refreshes waiting imports.
    State,
}

pub(crate) struct ImportFlow {
    job: ImportSession,
    /// This tab's hold on the app-wide import slot (`slot`).
    permit: Option<Arc<ImportPermit>>,
    slot: Arc<AtomicBool>,
    /// Set when the tab closes; the running conversion stops.
    cancel: Arc<AtomicBool>,
    /// Cancel was pressed: the running conversion is winding down (#950).
    stopping: bool,
    /// The file being converted, kept for OCR consent and retries.
    source: Option<PathBuf>,
    /// An opened PDF or DOCX converts once the slot is free.
    auto_pending: bool,
    /// The running conversion started by itself, not from Convert.
    automatic: bool,
    ocr_required: Option<Vec<u32>>,
    /// Every page of the PDF needs recognition, so the card says "all".
    ocr_all_pages: bool,
    /// This document's last OCR run recognized nothing; OCR itself still works.
    ocr_error: Option<String>,
    /// Setup or a run of this document's OCR model failed, when that is not
    /// the Settings model (whose state the tabs share).
    ocr_failure: Option<String>,
    /// What detection saw when the card appeared.
    signals: Option<recognition::Signals>,
    /// Recognition languages picked on the card, for this document only.
    pick: Option<recognition::Choice>,
    /// The model this tab is setting up, and whether it put the shared OCR
    /// state into Installing (it must replace that state on finish).
    setup: Option<(OcrModel, bool)>,
    ocr_notice_dismissed: bool,
    setup_error_dismissed: bool,
    /// The source is password-protected: the card asks for its password.
    /// A typed password goes to one run; only what it unlocked is kept.
    password_prompt: Option<PasswordPrompt>,
    /// The tab's unlock of its locked source: every later run (OCR, Try
    /// again) and the preview take it instead of asking again. Memory only,
    /// never in the session; dropped when the tab closes (#1024 decision 6).
    unlock: Option<Unlock>,
    /// What the finished run for a generation left of the unlock, adopted
    /// with its result.
    finished_unlock: Option<(u64, Option<Unlock>)>,
    /// Keyboard input source languages; tests inject their own.
    input_languages: fn() -> Vec<recognition::Language>,
}

impl ImportFlow {
    pub(crate) fn new(
        slot: Arc<AtomicBool>,
        input_languages: fn() -> Vec<recognition::Language>,
    ) -> Self {
        Self {
            job: ImportSession::default(),
            permit: None,
            slot,
            cancel: Arc::new(AtomicBool::new(false)),
            stopping: false,
            source: None,
            auto_pending: false,
            automatic: false,
            ocr_required: None,
            ocr_all_pages: false,
            ocr_error: None,
            ocr_failure: None,
            signals: None,
            pick: None,
            setup: None,
            ocr_notice_dismissed: false,
            setup_error_dismissed: false,
            password_prompt: None,
            unlock: None,
            finished_unlock: None,
            input_languages,
        }
    }

    // ---- What the card and the sidebar show ------------------------------

    pub(crate) fn busy(&self) -> bool {
        self.job.busy()
    }

    pub(crate) fn recognizing(&self) -> bool {
        self.job.recognizing()
    }

    /// A conversion waits for OCR setup to finish.
    pub(crate) fn setting_up(&self) -> bool {
        self.job.has_ocr_continuation()
    }

    /// Another tab (or this one) holds the app-wide import slot.
    pub(crate) fn slot_busy(&self) -> bool {
        self.slot.load(Ordering::Relaxed)
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Cancel was pressed and the running conversion has not ended yet.
    pub(crate) fn stopping(&self) -> bool {
        self.stopping && self.job.busy()
    }

    pub(crate) fn auto_pending(&self) -> bool {
        self.auto_pending
    }

    pub(crate) fn ocr_required(&self) -> Option<&[u32]> {
        self.ocr_required.as_deref()
    }

    pub(crate) fn ocr_all_pages(&self) -> bool {
        self.ocr_all_pages
    }

    pub(crate) fn ocr_error(&self) -> Option<&String> {
        self.ocr_error.as_ref()
    }

    pub(crate) fn ocr_failure(&self) -> Option<&String> {
        self.ocr_failure.as_ref()
    }

    /// The card asks for the source's password.
    pub(crate) fn password_prompt(&self) -> Option<PasswordPrompt> {
        self.password_prompt
    }

    /// The tab's unlock of its locked source: the unlocked bytes of an
    /// Office document, or a PDF's password, for whatever opens it again.
    pub(crate) fn unlock(&self) -> Option<&Unlock> {
        self.unlock.as_ref()
    }

    /// A password typed on the Original's card opened the source:
    /// `unlock` becomes the tab's, for its preview and every later run, as
    /// one the card's conversion left would. A closed tab keeps nothing.
    pub(crate) fn keep_unlock(&mut self, unlock: Unlock) {
        if !self.cancelled() {
            self.unlock = Some(unlock);
        }
    }

    /// A password typed on the card can be tried now: nothing else runs
    /// and the slot is free.
    pub(crate) fn can_unlock(&self) -> bool {
        self.password_prompt.is_some()
            && self.source.is_some()
            && !self.job.busy()
            && !self.slot_busy()
            && !self.cancelled()
    }

    pub(crate) fn ocr_notice_dismissed(&self) -> bool {
        self.ocr_notice_dismissed
    }

    pub(crate) fn setup_error_dismissed(&self) -> bool {
        self.setup_error_dismissed
    }

    pub(crate) fn dismiss_ocr_notice(&mut self) {
        self.ocr_notice_dismissed = true;
    }

    pub(crate) fn dismiss_setup_error(&mut self) {
        self.setup_error_dismissed = true;
    }

    // ---- Recognition languages ----------------------------------

    /// This document's recognition languages: the card's pick, else the
    /// detected default; `fallback` is the Settings model.
    pub(crate) fn choice(&self, fallback: Option<OcrModel>) -> recognition::Choice {
        if let Some(pick) = &self.pick {
            return pick.clone();
        }
        self.detected(fallback)
    }

    pub(crate) fn detected(&self, fallback: Option<OcrModel>) -> recognition::Choice {
        recognition::detect(
            &self.signals.clone().unwrap_or_default(),
            &ocr::offered(),
            ocr::languages(fallback.unwrap_or_default()),
        )
    }

    /// The bundle that reads this document's recognition languages.
    pub(crate) fn model(&self, fallback: Option<OcrModel>) -> OcrModel {
        ocr::plan(&self.choice(fallback).languages).0
    }

    /// Gather detection signals for `path` once its card appears.
    fn observe(&mut self, path: &Path, text: Vec<recognition::Language>) {
        self.signals = Some(recognition::Signals {
            text,
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            input_sources: (self.input_languages)(),
        });
    }

    /// Pick recognition languages on the card. The pick holds until the
    /// document converts; picking the detected list again keeps its
    /// explanation, frozen so a later Settings change cannot move it.
    pub(crate) fn pick(
        &mut self,
        languages: Vec<recognition::Language>,
        fallback: Option<OcrModel>,
    ) {
        let detected = self.detected(fallback);
        self.pick = Some(if detected.languages == languages {
            detected
        } else {
            recognition::Choice::picked(languages)
        });
        self.ocr_failure = None;
    }

    #[cfg(test)]
    pub(crate) fn picked(&self) -> Option<&recognition::Choice> {
        self.pick.as_ref()
    }

    // ---- Test setup -----------------------------------------------------------

    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn show_ocr_fixture(&mut self, generation: u64) {
        // Render evidence uses a fixed signal, never the host's keyboards.
        self.input_languages = Vec::new;
        self.job.complete(
            generation,
            Err(ImportError::NeedsOcr(
                "Scanned agreement.pdf".into(),
                vec![1],
                Vec::new(),
                1,
            )),
        );
    }

    /// The card asking for `name`'s password, first or after a `wrong` one.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn show_password_fixture(&mut self, generation: u64, name: &str, wrong: bool) {
        self.source = Some(name.into());
        self.job.complete(
            generation,
            Err(if wrong {
                ImportError::WrongPassword
            } else {
                ImportError::PasswordRequired
            }),
        );
    }

    #[cfg(test)]
    pub(crate) fn job(&self) -> &ImportSession {
        &self.job
    }

    #[cfg(test)]
    pub(crate) fn job_mut(&mut self) -> &mut ImportSession {
        &mut self.job
    }

    #[cfg(test)]
    pub(crate) fn set_pick(&mut self, pick: Option<recognition::Choice>) {
        self.pick = pick;
    }

    #[cfg(test)]
    pub(crate) fn set_setup(&mut self, setup: Option<(OcrModel, bool)>) {
        self.setup = setup;
    }

    #[cfg(test)]
    pub(crate) fn set_source(&mut self, source: Option<PathBuf>) {
        self.source = source;
    }

    #[cfg(test)]
    pub(crate) fn require_ocr(&mut self, pages: Option<Vec<u32>>) {
        self.ocr_required = pages;
    }

    // ---- Conversions --------------------------------------------------------

    /// An opened PDF or DOCX is to convert by itself.
    pub(crate) fn queue_auto(&mut self, pending: bool) {
        self.auto_pending = pending;
    }

    /// Convert pressed on `source`: the conversion is the user's own.
    pub(crate) fn convert_by_hand(&mut self, source: Option<PathBuf>) -> Option<ImportEffect> {
        self.automatic = false;
        self.auto_pending = false;
        Some(Self::convert(source?))
    }

    fn convert(path: PathBuf) -> ImportEffect {
        ImportEffect::Convert {
            path,
            skip_ocr: false,
            allow_ocr: false,
        }
    }

    /// The waiting automatic conversion of `source` may start now.
    pub(crate) fn auto_convert(&mut self, source: Option<PathBuf>) -> Option<ImportEffect> {
        if !self.auto_pending || self.job.busy() || self.slot_busy() || self.cancelled() {
            return None;
        }
        let source = source?;
        self.automatic = true;
        self.auto_pending = false;
        Some(Self::convert(source))
    }

    /// Start converting `path` with `installed` OCR, or none. Nothing when
    /// a job runs, the slot is taken, or the job refuses to start.
    pub(crate) fn begin(
        &mut self,
        path: PathBuf,
        installed: Option<ocr::Installed>,
        skip_ocr: bool,
    ) -> Option<ImportEffect> {
        self.run(path, None, installed, skip_ocr)
    }

    /// Unlock on the card: convert the source with `password`, which goes
    /// with the run; what it unlocks stays with the tab. Nothing unless
    /// [`Self::can_unlock`].
    pub(crate) fn try_password(&mut self, password: Password) -> Option<ImportEffect> {
        if !self.can_unlock() {
            return None;
        }
        let path = self.source.clone()?;
        self.automatic = false;
        let unlock = Unlock::Password {
            path: path.clone(),
            password,
        };
        self.run(path, Some(unlock), None, false)
    }

    /// Run a conversion of `path` with `typed`, a password from the card,
    /// else the tab's unlock.
    fn run(
        &mut self,
        path: PathBuf,
        typed: Option<Unlock>,
        installed: Option<ocr::Installed>,
        skip_ocr: bool,
    ) -> Option<ImportEffect> {
        if self.job.busy() {
            return None;
        }
        if self.permit.is_none() {
            self.permit = ImportPermit::acquire(&self.slot);
        }
        let permit = self.permit.clone()?;
        if !self.job.begin(installed.is_some()) {
            return None;
        }
        self.source = Some(path.clone());
        self.ocr_error = None;
        Some(ImportEffect::Run {
            path,
            unlock: typed.or_else(|| self.unlock.clone()),
            installed,
            skip_ocr,
            permit,
            cancel: self.cancel.clone(),
        })
    }

    /// The conversion for `generation` finished in the background, leaving
    /// `unlock` of the one it took. A closed tab keeps nothing.
    pub(crate) fn complete(
        &mut self,
        generation: u64,
        result: Result<Imported, ImportError>,
        unlock: Option<Unlock>,
    ) {
        self.finished_unlock = (!self.cancelled()).then_some((generation, unlock));
        self.job.complete(generation, result);
    }

    /// Take a finished conversion while no dialog is open. `shared` says
    /// whether this document recognizes with the Settings model.
    pub(crate) fn resume(
        &mut self,
        generation: u64,
        prompting: bool,
        shared: impl FnOnce(&Self) -> bool,
    ) -> Option<Vec<ImportEffect>> {
        let completion = self.job.take_ready(generation, prompting)?;
        let finished_unlock = self.finished_unlock.take();
        // A run that finishes after Cancel was accepted (the converter
        // returned before checking the flag) still ends cancelled.
        let stopped = std::mem::take(&mut self.stopping);
        let mut effects = Vec::new();
        if let ImportCompletion::Ready(result) = completion {
            // What opens the source from now on: the run's unlock, or
            // nothing (not locked, a wrong password, a changed file).
            if let Some((finished, unlock)) = finished_unlock
                && finished == generation
            {
                self.unlock = unlock;
            }
            let result = if stopped {
                Err(ImportError::Cancelled)
            } else {
                *result
            };
            self.password_prompt = None;
            match result {
                Ok(imported) => effects.push(ImportEffect::Request(imported)),
                // The card asks for the password, or again after a wrong
                // one; the field takes focus.
                Err(ImportError::PasswordRequired) => {
                    self.password_prompt = Some(PasswordPrompt::Asking);
                    effects.push(ImportEffect::FocusCard);
                }
                Err(ImportError::WrongPassword) => {
                    self.password_prompt = Some(PasswordPrompt::Wrong);
                    effects.push(ImportEffect::FocusCard);
                }
                Err(ImportError::OcrFailed(error)) => {
                    let error = error.to_string();
                    if shared(self) {
                        effects.push(ImportEffect::SharedOcrFailed(error.clone()));
                        effects.push(ImportEffect::SharedOcrChanged);
                    } else {
                        self.ocr_failure = Some(error.clone());
                    }
                    effects.push(ImportEffect::Error(error));
                    if let Some(path) = self.source.clone() {
                        effects.extend(self.await_consent(path));
                    }
                }
                Err(error @ ImportError::OcrNoText) => {
                    let error = error.to_string();
                    self.ocr_error = Some(error.clone());
                    effects.push(ImportEffect::Error(error));
                    if let Some(path) = self.source.clone() {
                        effects.extend(self.await_consent(path));
                    }
                }
                Err(ImportError::NeedsOcr(path, pages, text, page_count)) => {
                    self.observe(&path, text);
                    self.ocr_all_pages = pages.len() as u32 >= page_count;
                    self.ocr_required = Some(pages);
                    effects.extend(self.await_consent(path));
                }
                Err(error) => effects.push(ImportEffect::Error(error.to_string())),
            }
        }
        if !self.job.busy() {
            self.permit = None;
            effects.push(ImportEffect::State);
        }
        Some(effects)
    }

    /// Consent lives in the source pane. Waiting for a decision must not
    /// hold the global conversion slot or interrupt another tab.
    fn await_consent(&mut self, path: PathBuf) -> [ImportEffect; 2] {
        if self.signals.is_none() {
            self.observe(&path, Vec::new());
        }
        self.source = Some(path);
        self.ocr_required.get_or_insert_with(Vec::new);
        self.ocr_notice_dismissed = false;
        self.job.finish();
        self.permit = None;
        [ImportEffect::FocusCard, ImportEffect::State]
    }

    /// The conversion replaced the document. Returns whether it started by
    /// itself, so the document counts as generated and unedited.
    pub(crate) fn imported(&mut self) -> bool {
        self.password_prompt = None;
        self.ocr_required = None;
        self.ocr_all_pages = false;
        self.ocr_error = None;
        self.ocr_failure = None;
        self.signals = None;
        self.pick = None;
        self.automatic
    }

    /// The card's Recognize (`skip` false) or Skip OCR, for the document
    /// at `generation`. Nothing while a job, setup or another conversion is
    /// in the way (`ocr_busy` is the shared OCR state). Recognizing freezes
    /// the languages chosen now, then converts when `installed` (this
    /// document's model is ready), else sets the model up unless another
    /// model job runs (`model_busy`).
    pub(crate) fn consent(
        &mut self,
        skip: bool,
        ocr_busy: bool,
        fallback: Option<OcrModel>,
        generation: u64,
        installed: bool,
        model_busy: bool,
    ) -> Option<ImportEffect> {
        if self.job.busy() || self.slot_busy() || ocr_busy || self.ocr_required.is_none() {
            return None;
        }
        let path = self.source.clone()?;
        self.automatic = false;
        if !skip && self.pick.is_none() {
            // Running OCR keeps this choice: a Settings change during
            // setup or recognition no longer moves it.
            self.pick = Some(self.detected(fallback));
        }
        if skip || installed {
            return Some(ImportEffect::Convert {
                path,
                skip_ocr: skip,
                allow_ocr: !skip,
            });
        }
        if !ocr::SUPPORTED {
            return None;
        }
        if model_busy {
            return Some(ImportEffect::Error(
                "Another model job is running. Retry when it finishes.".into(),
            ));
        }
        self.defer_for_setup(generation, path)
            .then_some(ImportEffect::SetUp)
    }

    // ---- OCR setup on the card ----------------------------------------------

    /// Hold the slot and wait for setup to finish before converting `path`.
    /// False when another conversion holds the slot.
    fn defer_for_setup(&mut self, generation: u64, path: PathBuf) -> bool {
        self.permit = ImportPermit::acquire(&self.slot);
        if self.permit.is_none() {
            return false;
        }
        self.job.defer_for_setup(generation, path);
        true
    }

    /// A setup is about to start; any earlier failure no longer shows.
    pub(crate) fn begin_setup(&mut self) {
        self.ocr_failure = None;
        self.setup = None;
    }

    /// Setup of `model` started; `shared` when it is the Settings model,
    /// whose state now reads Installing.
    pub(crate) fn setup_started(&mut self, model: OcrModel, shared: bool) {
        self.setup = Some((model, shared));
    }

    /// Setup of a model ended with `result`, for the document now at
    /// `generation`; `settings_model` is the Settings one. Only the Settings
    /// model's outcome is shared with every tab; another model's failure
    /// stays with this document. Settings may have changed during setup: a
    /// tab that set the shared state to Installing replaces it either way.
    /// A conversion waiting on the setup resumes, or returns to consent.
    pub(crate) fn finish_setup(
        &mut self,
        result: Result<ocr::Installed, String>,
        generation: u64,
        settings_model: Option<OcrModel>,
    ) -> Vec<ImportEffect> {
        let (model, claimed) = self.setup.take().unwrap_or_else(|| {
            let model = match &result {
                Ok(installed) => installed.config.model,
                Err(_) => self.model(settings_model),
            };
            (model, settings_model == Some(model))
        });
        let shared = settings_model == Some(model);
        let mut effects = Vec::new();
        match &result {
            Ok(installed) if shared => {
                effects.push(ImportEffect::SharedOcrReady(installed.clone()));
            }
            Err(error) if shared => effects.push(ImportEffect::SharedOcrFailed(error.clone())),
            Err(error) => self.ocr_failure = Some(error.clone()),
            Ok(_) => {}
        }
        if claimed && !shared {
            effects.push(ImportEffect::SharedOcrKnown);
        }
        match result {
            Ok(installed) => {
                if let Some(path) = self.job.resume_after_setup(generation) {
                    effects.push(ImportEffect::ConvertWith { path, installed });
                }
            }
            Err(_) => {
                self.job.finish();
                if let Some(path) = self.job.resume_after_setup(generation) {
                    effects.extend(self.await_consent(path));
                }
            }
        }
        // A conversion resuming above takes the slot again as it starts.
        if !self.job.busy() {
            self.permit = None;
        }
        if shared || claimed {
            effects.push(ImportEffect::SharedOcrChanged);
        }
        effects.push(ImportEffect::State);
        effects
    }

    /// Cancel on the card: the running conversion sees its flag and ends
    /// `Cancelled`; the tab stays and can convert again, so later runs get
    /// a fresh flag (#950).
    pub(crate) fn stop(&mut self) {
        if !self.job.busy() || self.stopping {
            return;
        }
        self.stopping = true;
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
    }

    /// The tab is closing: a running conversion stops, the slot goes, and
    /// the unlock is dropped (wiped once a running job lets go of it).
    pub(crate) fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.permit = None;
        self.unlock = None;
        self.finished_unlock = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_import::import::OcrFailure;
    use mdoc_store::settings::OcrConfig;

    fn new_flow() -> (ImportFlow, Arc<AtomicBool>) {
        let slot = Arc::new(AtomicBool::new(false));
        (ImportFlow::new(slot.clone(), Vec::new), slot)
    }

    fn installed(model: OcrModel) -> ocr::Installed {
        ocr::Installed {
            config: OcrConfig {
                model,
                ..OcrConfig::default()
            },
            models: "/models".into(),
            pdfium: "/pdfium".into(),
            onnx: "/onnx".into(),
        }
    }

    /// Start converting `path`, dropping the run's own permit as the
    /// worker does when it ends.
    fn start(flow: &mut ImportFlow, path: &str, ocr: Option<ocr::Installed>) {
        let Some(ImportEffect::Run { permit, .. }) = flow.begin(path.into(), ocr, false) else {
            panic!("the conversion starts");
        };
        drop(permit);
    }

    fn names(effects: &[ImportEffect]) -> Vec<&'static str> {
        effects
            .iter()
            .map(|effect| match effect {
                ImportEffect::Convert { .. } => "Convert",
                ImportEffect::ConvertWith { .. } => "ConvertWith",
                ImportEffect::Run { .. } => "Run",
                ImportEffect::SetUp => "SetUp",
                ImportEffect::Request(_) => "Request",
                ImportEffect::SharedOcrReady(_) => "SharedOcrReady",
                ImportEffect::SharedOcrFailed(_) => "SharedOcrFailed",
                ImportEffect::SharedOcrKnown => "SharedOcrKnown",
                ImportEffect::SharedOcrChanged => "SharedOcrChanged",
                ImportEffect::Error(_) => "Error",
                ImportEffect::FocusCard => "FocusCard",
                ImportEffect::State => "State",
            })
            .collect()
    }

    #[test]
    fn one_conversion_holds_the_slot_until_it_ends() {
        let (mut flow, slot) = new_flow();
        let Some(ImportEffect::Run { permit, path, .. }) =
            flow.begin("/in/a.pdf".into(), None, false)
        else {
            panic!("the conversion starts");
        };
        assert_eq!(path, PathBuf::from("/in/a.pdf"));
        assert!(flow.busy() && slot.load(Ordering::Relaxed));
        assert!(flow.begin("/in/b.pdf".into(), None, false).is_none());
        // Another tab finds the slot taken.
        let mut other = ImportFlow::new(slot.clone(), Vec::new);
        assert!(other.begin("/in/c.pdf".into(), None, false).is_none());
        drop(permit);
        // A dialog holds the completion back.
        flow.complete(1, Err(ImportError::Cancelled), None);
        assert!(flow.resume(1, true, |_| false).is_none());
        let effects = flow.resume(1, false, |_| false).unwrap();
        assert_eq!(names(&effects), ["Error", "State"]);
        assert!(!flow.busy() && !slot.load(Ordering::Relaxed));
        assert_eq!(flow.source, Some(PathBuf::from("/in/a.pdf")));
    }

    /// A conversion that succeeds or fails after Cancel was accepted still
    /// ends cancelled: the stop wins over a late result (#950 review).
    #[test]
    fn a_late_result_after_cancel_still_ends_cancelled() {
        let late_results = [
            Ok(crate::test_support::converted("/in/a.docx".into())),
            Err(ImportError::OcrNoText),
            Err(ImportError::Encrypted),
        ];
        for late in late_results {
            let (mut flow, slot) = new_flow();
            let Some(ImportEffect::Run { permit, .. }) =
                flow.begin("/in/a.docx".into(), None, false)
            else {
                panic!("the conversion starts");
            };
            flow.stop();
            assert!(
                slot.load(Ordering::Relaxed),
                "the slot stays held until the run exits"
            );
            drop(permit);
            flow.complete(1, late, None);
            let effects = flow.resume(1, false, |_| false).unwrap();
            let [ImportEffect::Error(error), ImportEffect::State] = &effects[..] else {
                panic!("{:?}", names(&effects));
            };
            assert_eq!(error, &ImportError::Cancelled.to_string());
            assert!(!slot.load(Ordering::Relaxed));
        }
    }

    /// Cancel on the card stops this run only: it ends `Cancelled` as an
    /// error, and the next Convert runs with a flag that isn't set (#950).
    #[test]
    fn cancel_stops_the_run_and_the_tab_can_convert_again() {
        let (mut flow, slot) = new_flow();
        let Some(ImportEffect::Run { permit, cancel, .. }) =
            flow.begin("/in/a.docx".into(), None, false)
        else {
            panic!("the conversion starts");
        };
        assert!(!flow.stopping());
        flow.stop();
        assert!(
            cancel.load(Ordering::Relaxed),
            "the running conversion sees the flag"
        );
        assert!(flow.stopping() && flow.busy());
        assert!(!flow.cancelled(), "the tab itself is not closing");
        drop(permit);
        flow.complete(1, Err(ImportError::Cancelled), None);
        let effects = flow.resume(1, false, |_| false).unwrap();
        assert_eq!(names(&effects), ["Error", "State"]);
        assert!(!flow.stopping() && !flow.busy() && !slot.load(Ordering::Relaxed));
        let Some(ImportEffect::Run { cancel, .. }) = flow.begin("/in/a.docx".into(), None, false)
        else {
            panic!("Try again starts a new conversion");
        };
        assert!(!cancel.load(Ordering::Relaxed));
        // Nothing to stop once a run has ended.
        let (mut idle, _) = new_flow();
        idle.stop();
        assert!(!idle.stopping() && !idle.cancelled());
    }

    /// A locked document: the card asks without holding the slot, Unlock
    /// runs with the password, a wrong one asks again, and the right one
    /// imports. Any other ending puts the card away.
    #[test]
    fn a_locked_document_asks_for_its_password_until_it_imports() {
        let (mut flow, slot) = new_flow();
        assert!(!flow.can_unlock(), "nothing asks yet");
        start(&mut flow, "/in/locked.docx", None);
        flow.complete(1, Err(ImportError::PasswordRequired), None);
        let effects = flow.resume(1, false, |_| false).unwrap();
        assert_eq!(names(&effects), ["FocusCard", "State"]);
        assert_eq!(flow.password_prompt(), Some(PasswordPrompt::Asking));
        assert!(!flow.busy() && !slot.load(Ordering::Relaxed));
        for (result, prompt) in [
            (Err(ImportError::WrongPassword), Some(PasswordPrompt::Wrong)),
            (Err(ImportError::Damaged), None),
        ] {
            assert!(flow.can_unlock());
            let Some(ImportEffect::Run {
                path,
                unlock,
                permit,
                skip_ocr,
                installed,
                ..
            }) = flow.try_password(Password::new("guess".into()))
            else {
                panic!("Unlock runs the conversion");
            };
            assert_eq!(path, PathBuf::from("/in/locked.docx"));
            assert!(
                unlock.as_ref().and_then(Unlock::password).is_some()
                    && !skip_ocr
                    && installed.is_none()
            );
            assert!(
                flow.try_password(Password::new("again".into())).is_none(),
                "one run at a time"
            );
            drop(permit);
            flow.complete(1, result, None);
            let effects = flow.resume(1, false, |_| false).unwrap();
            assert_eq!(flow.password_prompt(), prompt);
            if prompt.is_some() {
                assert_eq!(names(&effects), ["FocusCard", "State"]);
            } else {
                assert_eq!(names(&effects), ["Error", "State"]);
                assert!(!flow.can_unlock());
            }
        }
        // Asked again (Try again converts without a password), then the
        // right password imports.
        start(&mut flow, "/in/locked.docx", None);
        flow.complete(1, Err(ImportError::PasswordRequired), None);
        flow.resume(1, false, |_| false).unwrap();
        let Some(ImportEffect::Run { permit, .. }) =
            flow.try_password(Password::new("right".into()))
        else {
            panic!("Unlock runs the conversion");
        };
        drop(permit);
        flow.complete(
            1,
            Ok(crate::test_support::converted("/in/locked.docx".into())),
            None,
        );
        let effects = flow.resume(1, false, |_| false).unwrap();
        assert_eq!(names(&effects), ["Request", "State"]);
        assert_eq!(flow.password_prompt(), None);
        assert!(!slot.load(Ordering::Relaxed));
    }

    /// The run that took `unlock`'s password, for the tab's source.
    fn password_in(effect: Option<ImportEffect>) -> Option<String> {
        let Some(ImportEffect::Run { unlock, permit, .. }) = effect else {
            panic!("the conversion runs");
        };
        drop(permit);
        unlock
            .as_ref()
            .and_then(Unlock::password)
            .map(|password| password.expose().to_owned())
    }

    /// The tab keeps what unlocked its source (#1024 decision 6): OCR
    /// consent, a failed recognition and Try again after Cancel all run
    /// with it and never ask, a stale run doesn't change it, a run that
    /// opened nothing drops it, and closing the tab drops it for good,
    /// even from a run that ends afterwards.
    #[test]
    fn the_tab_s_unlock_goes_with_every_later_run_until_the_tab_closes() {
        const PDF: &str = "/in/locked.pdf";
        let held = || {
            Some(Unlock::Password {
                path: PDF.into(),
                password: Password::new("right".into()),
            })
        };
        let (mut flow, _) = new_flow();
        start(&mut flow, PDF, None);
        flow.complete(1, Err(ImportError::PasswordRequired), None);
        flow.resume(1, false, |_| false).unwrap();
        assert_eq!(
            password_in(flow.try_password(Password::new("right".into()))).as_deref(),
            Some("right")
        );
        // The password opened it; its scanned pages wait for consent.
        flow.complete(
            1,
            Err(ImportError::NeedsOcr(PDF.into(), vec![1], Vec::new(), 1)),
            held(),
        );
        flow.resume(1, false, |_| false).unwrap();
        assert_eq!(flow.password_prompt(), None);
        assert!(flow.unlock().is_some());

        // Recognize: the run takes the tab's unlock.
        let Some(ImportEffect::Convert {
            path, allow_ocr, ..
        }) = flow.consent(false, false, None, 1, true, false)
        else {
            panic!("Recognize converts");
        };
        assert!(allow_ocr);
        let ocr = Some(installed(OcrModel::default()));
        let run = flow.begin(path, ocr.clone(), false);
        assert_eq!(password_in(run).as_deref(), Some("right"));
        flow.complete(
            1,
            Err(ImportError::OcrFailed(OcrFailure::Engine("stopped".into()))),
            held(),
        );
        flow.resume(1, false, |_| false).unwrap();
        assert!(flow.ocr_required().is_some() && flow.unlock().is_some());
        // Recognize again, Cancel, then Try again: still no asking.
        let run = flow.begin(PDF.into(), ocr.clone(), false);
        assert_eq!(password_in(run).as_deref(), Some("right"));
        flow.stop();
        flow.complete(1, Ok(crate::test_support::converted(PDF.into())), held());
        flow.resume(1, false, |_| false).unwrap();
        let run = flow.begin(PDF.into(), None, false);
        assert_eq!(password_in(run).as_deref(), Some("right"));
        // A run for a replaced document leaves the unlock as it was.
        flow.complete(2, Err(ImportError::PasswordRequired), None);
        flow.resume(1, false, |_| false).unwrap();
        assert!(flow.unlock().is_some() && flow.password_prompt().is_none());
        // The file changed: the run opened nothing, so the card asks.
        start(&mut flow, PDF, None);
        flow.complete(1, Err(ImportError::PasswordRequired), None);
        flow.resume(1, false, |_| false).unwrap();
        assert!(flow.unlock().is_none());
        assert_eq!(flow.password_prompt(), Some(PasswordPrompt::Asking));

        // Unlocked again, then the tab closes while a run is out.
        password_in(flow.try_password(Password::new("right".into())));
        flow.complete(1, Ok(crate::test_support::converted(PDF.into())), held());
        flow.resume(1, false, |_| false).unwrap();
        assert!(flow.unlock().is_some());
        let run = flow.begin(PDF.into(), None, false);
        assert_eq!(password_in(run).as_deref(), Some("right"));
        flow.cancel();
        assert!(flow.unlock().is_none());
        flow.complete(1, Err(ImportError::Cancelled), held());
        flow.resume(1, false, |_| false).unwrap();
        assert!(flow.unlock().is_none(), "a closed tab keeps nothing");
    }

    #[test]
    fn a_stale_conversion_only_frees_the_slot() {
        let (mut flow, slot) = new_flow();
        start(&mut flow, "/in/a.pdf", None);
        flow.complete(1, Err(ImportError::Cancelled), None);
        let effects = flow.resume(2, false, |_| false).unwrap();
        assert_eq!(names(&effects), ["State"]);
        assert!(!slot.load(Ordering::Relaxed));
    }

    #[test]
    fn pages_without_text_ask_for_consent_without_holding_the_slot() {
        let (mut flow, slot) = new_flow();
        start(&mut flow, "/in/scan.pdf", None);
        flow.complete(
            1,
            Err(ImportError::NeedsOcr(
                "/in/scan.pdf".into(),
                vec![2, 5],
                Vec::new(),
                5,
            )),
            None,
        );
        let effects = flow.resume(1, false, |_| false).unwrap();
        assert_eq!(names(&effects), ["FocusCard", "State", "State"]);
        assert_eq!(flow.ocr_required(), Some(&[2, 5][..]));
        assert!(!flow.ocr_all_pages() && !flow.ocr_notice_dismissed());
        assert!(!slot.load(Ordering::Relaxed));
        // Not while the shared OCR state is busy.
        assert!(flow.consent(true, true, None, 1, false, false).is_none());
        // Skip converts without recognition.
        let Some(ImportEffect::Convert {
            skip_ocr: true,
            allow_ocr: false,
            ..
        }) = flow.consent(true, false, None, 1, false, false)
        else {
            panic!("Skip OCR converts");
        };
        assert!(flow.picked().is_none(), "skipping freezes no languages");
        // Recognize freezes the detected languages and converts with OCR.
        let Some(ImportEffect::Convert {
            path,
            skip_ocr: false,
            allow_ocr: true,
        }) = flow.consent(false, false, None, 1, true, false)
        else {
            panic!("Recognize converts with an installed model");
        };
        assert_eq!(path, PathBuf::from("/in/scan.pdf"));
        assert!(flow.picked().is_some());
        // Converting clears the card's state.
        assert!(!flow.imported());
        assert_eq!(flow.ocr_required(), None);
        assert!(flow.picked().is_none());
    }

    #[test]
    fn recognize_without_the_model_sets_it_up_unless_another_job_runs() {
        let (mut flow, slot) = new_flow();
        flow.source = Some("/in/scan.pdf".into());
        flow.ocr_required = Some(vec![1]);
        if !ocr::SUPPORTED {
            assert!(flow.consent(false, false, None, 1, false, false).is_none());
            return;
        }
        let Some(ImportEffect::Error(error)) = flow.consent(false, false, None, 1, false, true)
        else {
            panic!("another model job blocks setup");
        };
        assert_eq!(
            error,
            "Another model job is running. Retry when it finishes."
        );
        assert!(!slot.load(Ordering::Relaxed));
        assert!(matches!(
            flow.consent(false, false, None, 1, false, false),
            Some(ImportEffect::SetUp)
        ));
        assert!(flow.setting_up() && slot.load(Ordering::Relaxed));
        // While it waits, the card refuses another choice.
        assert!(flow.consent(true, false, None, 1, true, false).is_none());
    }

    #[test]
    fn ocr_failures_go_to_the_shared_state_or_this_document() {
        let failure = || {
            Err(ImportError::OcrFailed(OcrFailure::Engine(
                "the engine stopped".into(),
            )))
        };
        // The Settings model: every tab hears it, the document asks again.
        let (mut flow, _) = new_flow();
        flow.source = Some("/in/scan.pdf".into());
        start(
            &mut flow,
            "/in/scan.pdf",
            Some(installed(OcrModel::default())),
        );
        flow.complete(1, failure(), None);
        let effects = flow.resume(1, false, |_| true).unwrap();
        assert_eq!(
            names(&effects),
            [
                "SharedOcrFailed",
                "SharedOcrChanged",
                "Error",
                "FocusCard",
                "State",
                "State"
            ]
        );
        assert!(flow.ocr_failure().is_none());
        // Another model: the failure stays with this document.
        let (mut flow, _) = new_flow();
        start(
            &mut flow,
            "/in/scan.pdf",
            Some(installed(OcrModel::default())),
        );
        flow.complete(1, failure(), None);
        let effects = flow.resume(1, false, |_| false).unwrap();
        assert_eq!(names(&effects), ["Error", "FocusCard", "State", "State"]);
        assert!(flow.ocr_failure().is_some());
        // Recognition that found nothing is this document's own error, and
        // a new run clears it.
        start(
            &mut flow,
            "/in/scan.pdf",
            Some(installed(OcrModel::default())),
        );
        flow.complete(1, Err(ImportError::OcrNoText), None);
        let effects = flow.resume(1, false, |_| true).unwrap();
        assert_eq!(names(&effects)[0], "Error");
        assert!(flow.ocr_error().is_some());
        start(&mut flow, "/in/scan.pdf", None);
        assert!(flow.ocr_error().is_none());
    }

    #[test]
    fn automatic_conversion_starts_once_the_slot_is_free() {
        let (mut flow, slot) = new_flow();
        flow.queue_auto(true);
        slot.store(true, Ordering::Relaxed);
        assert!(flow.auto_convert(Some("/in/a.docx".into())).is_none());
        slot.store(false, Ordering::Relaxed);
        assert!(flow.auto_convert(None).is_none());
        assert!(flow.auto_pending());
        assert!(matches!(
            flow.auto_convert(Some("/in/a.docx".into())),
            Some(ImportEffect::Convert {
                skip_ocr: false,
                allow_ocr: false,
                ..
            })
        ));
        assert!(!flow.auto_pending());
        assert!(
            flow.imported(),
            "an automatic conversion counts as generated"
        );
        assert!(flow.convert_by_hand(None).is_none());
        assert!(!flow.imported());
        flow.queue_auto(true);
        assert!(flow.convert_by_hand(Some("/in/a.docx".into())).is_some());
        assert!(!flow.auto_pending());
        flow.queue_auto(true);
        flow.cancel();
        assert!(flow.auto_convert(Some("/in/a.docx".into())).is_none());
    }

    #[test]
    fn a_finished_setup_resumes_its_conversion_or_returns_to_consent() {
        let model = OcrModel::default();
        let (mut flow, slot) = new_flow();
        flow.source = Some("/in/scan.pdf".into());
        flow.ocr_required = Some(vec![1]);
        assert!(flow.defer_for_setup(3, "/in/scan.pdf".into()));
        flow.begin_setup();
        flow.setup_started(model, true);
        let effects = flow.finish_setup(Ok(installed(model)), 3, Some(model));
        assert_eq!(
            names(&effects),
            ["SharedOcrReady", "ConvertWith", "SharedOcrChanged", "State"]
        );
        assert!(
            !slot.load(Ordering::Relaxed),
            "the conversion takes it again"
        );
        // The document changed meanwhile: the old conversion does not resume.
        assert!(flow.defer_for_setup(3, "/in/scan.pdf".into()));
        let effects = flow.finish_setup(Ok(installed(model)), 4, Some(model));
        assert_eq!(
            names(&effects),
            ["SharedOcrReady", "SharedOcrChanged", "State"]
        );
        // Another model failed: the failure stays here, the card asks again.
        assert!(flow.defer_for_setup(3, "/in/scan.pdf".into()));
        let effects = flow.finish_setup(Err("download failed".into()), 3, None);
        assert_eq!(names(&effects), ["FocusCard", "State", "State"]);
        assert_eq!(
            flow.ocr_failure().map(String::as_str),
            Some("download failed")
        );
        assert!(!flow.busy() && !slot.load(Ordering::Relaxed));
        // A tab that claimed the shared state for a model Settings no longer
        // uses gives it back.
        flow.begin_setup();
        assert!(flow.ocr_failure().is_none());
        flow.setup_started(model, true);
        let effects = flow.finish_setup(Err("download failed".into()), 3, None);
        assert_eq!(
            names(&effects),
            ["SharedOcrKnown", "SharedOcrChanged", "State"]
        );
    }
}
