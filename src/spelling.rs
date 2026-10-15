//! Spellcheck in the document view (ADR 0036): background checks with a
//! per-document line cache, the session-only per-document switch, the toolbar
//! indicator and next/previous navigation. Checking and suggestions never run
//! on the UI thread; it only copies the text and the PII ranges.
use super::*;
use mdoc_editor::Diagnostic;
use mdoc_spell::{Accepted, Exclusions, LineCache, Options, Speller};
use std::{collections::HashSet, ops::Range, time::Duration};

/// Pause after an edit before rechecking, so a word being typed isn't
/// flagged halfway through.
const EDIT_DELAY: Duration = Duration::from_millis(300);

#[derive(Default)]
pub(crate) struct SpellState {
    /// The per-document switch. Session-only: never saved with the document
    /// or in app state.
    pub(crate) off_here: bool,
    /// The dictionaries are still parsing; nothing is flagged yet.
    pub(crate) loading: bool,
    pub(crate) menu_open: bool,
    /// Words ignored in this document ("Ignore in this document"). Session-only.
    ignored: HashSet<String>,
    ignored_generation: u64,
    /// (editor revision, options, accepted words, review revision) of the last
    /// scheduled check.
    scheduled: Option<(u64, Option<Options>, u64, u64)>,
    generation: u64,
    cache: LineCache,
    /// One worker owns the cache until it returns; only the latest request waits.
    running: bool,
    ready: bool,
    delay: Option<gpui::Task<()>>,
    worker: Option<gpui::Task<()>>,
    #[cfg(test)]
    snapshots: usize,
}

/// Spelling choices from Settings, or `None` when spellcheck is off there.
fn configured_options(preferences: &settings::Shared) -> Option<Options> {
    preferences.borrow().snapshot().ok()?.spelling.options()
}

/// Right-click menu actions for a flagged word, by [`EditorEvent::DiagnosticAction`] index.
const IGNORE: usize = 0;
const ADD_TO_DICTIONARY: usize = 1;

/// The editor's right-click menu for flagged words: suggestions computed in
/// the background, then Ignore and Add to dictionary.
pub(crate) fn install_editor_hooks(editor: &mut EditorState, preferences: settings::Shared) {
    editor.set_diagnostic_actions(vec![
        "Ignore in this document".into(),
        "Add to dictionary".into(),
    ]);
    editor.on_suggest(move |word, cx| {
        let options = configured_options(&preferences);
        let word = word.to_owned();
        cx.background_executor().spawn(async move {
            options.map_or_else(Vec::new, |options| Speller::load(options).suggest(&word))
        })
    });
}

impl DocumentView {
    /// What this document checks now, or `None` when it checks nothing.
    fn spelling_options(&self) -> Option<Options> {
        if self.spelling.off_here || self.source_only || self.loading || self.unavailable {
            return None;
        }
        configured_options(&self.preferences)
    }

    /// The user's dictionary plus this document's ignored words.
    fn accepted_words(&self) -> Accepted {
        let store = self.preferences.borrow();
        let user = &store.words;
        Accepted {
            words: Arc::new(
                user.words
                    .iter()
                    .chain(&self.spelling.ignored)
                    .cloned()
                    .collect(),
            ),
            // Either list changing must invalidate cached results.
            generation: (user.generation << 32) | self.spelling.ignored_generation,
        }
    }

    fn accepted_generation(&self) -> u64 {
        (self.preferences.borrow().words.generation << 32) | self.spelling.ignored_generation
    }

    /// Ignore a flagged word in this document, or add it to the user's
    /// dictionary; every instance stops being flagged at once.
    pub(crate) fn spelling_action(&mut self, action: usize, word: String, cx: &mut Context<Self>) {
        match action {
            IGNORE => {
                self.spelling.ignored.insert(word.clone());
                self.spelling.ignored_generation += 1;
            }
            ADD_TO_DICTIONARY => {
                let mut words = (*self.preferences.borrow().words.words).clone();
                words.insert(word.clone());
                if !settings::Store::set_words(&self.preferences, words, cx) {
                    cx.notify();
                    return;
                }
            }
            _ => return,
        }
        // Drop the word's squiggles now; the recheck confirms.
        let editor = self.editor.read(cx);
        let kept: Vec<Diagnostic> = editor
            .diagnostics()
            .iter()
            .filter(|d| editor.text().get(d.range.clone()) != Some(word.as_str()))
            .cloned()
            .collect();
        self.editor
            .update(cx, |editor, cx| editor.set_diagnostics(kept, cx));
        self.schedule_spellcheck(false, cx);
        cx.notify();
    }

    fn spellcheck_key(&self, cx: &App) -> (u64, Option<Options>, u64, u64) {
        (
            self.editor.read(cx).revision(),
            self.spelling_options(),
            self.accepted_generation(),
            self.pii.version,
        )
    }

    /// Debounce the request, not the copied document. One running check retains
    /// its worker/cache; superseding input replaces only the latest request.
    pub(crate) fn schedule_spellcheck(&mut self, after_edit: bool, cx: &mut Context<Self>) {
        let key = self.spellcheck_key(cx);
        if self.spelling.scheduled == Some(key) {
            return;
        }
        self.spelling.scheduled = Some(key);
        self.spelling.generation = self.spelling.generation.wrapping_add(1);
        self.spelling.delay = None;
        self.spelling.ready = false;
        let Some(options) = key.1 else {
            self.spelling.loading = false;
            if !self.editor.read(cx).diagnostics().is_empty() {
                self.editor
                    .update(cx, |editor, cx| editor.set_diagnostics(Vec::new(), cx));
            }
            return;
        };
        self.spelling.loading = !Speller::is_loaded(options);
        let generation = self.spelling.generation;
        if after_edit {
            self.spelling.delay = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(EDIT_DELAY).await;
                let _ = this.update(cx, |this, cx| {
                    if this.spelling.generation == generation {
                        this.spelling.delay = None;
                        this.spelling.ready = true;
                        this.start_spellcheck(cx);
                    }
                });
            }));
        } else {
            self.spelling.ready = true;
            // All transactions/metadata edits in this effect cycle settle
            // before taking a snapshot, including deferred editor events.
            let weak = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, cx| {
                    if this.spelling.generation == generation {
                        this.start_spellcheck(cx);
                    }
                });
            });
        }
    }

    fn start_spellcheck(&mut self, cx: &mut Context<Self>) {
        if self.spelling.running || !self.spelling.ready {
            return;
        }
        if self.spelling.scheduled != Some(self.spellcheck_key(cx)) {
            self.schedule_spellcheck(false, cx);
            return;
        }
        let Some((revision, Some(options), _, _)) = self.spelling.scheduled else {
            return;
        };
        self.spelling.ready = false;
        self.spelling.running = true;
        let text = self.editor.read(cx).text().to_owned();
        let review = &self.pii.review;
        let entities: Vec<Range<usize>> = review
            .candidates()
            .iter()
            .map(|o| o.range.clone())
            .collect();
        let aliases: Vec<Range<usize>> = review.applied().iter().map(|o| o.range.clone()).collect();
        let accepted = self.accepted_words();
        let mut cache = std::mem::take(&mut self.spelling.cache);
        let generation = self.spelling.generation;
        #[cfg(test)]
        {
            self.spelling.snapshots += 1;
        }
        let task = cx.background_executor().spawn(async move {
            let speller = Speller::load(options).with_accepted(accepted);
            let mut structural = mdoc_editor::spell_exclusions(&text);
            structural.extend(aliases);
            let exclusions = Exclusions {
                structural: &structural,
                entities: &entities,
            };
            let found = speller.check_cached(&text, exclusions, &mut cache);
            (found, cache)
        });
        self.spelling.worker = Some(cx.spawn(async move |this, cx| {
            let (found, cache) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.spelling.cache = cache;
                this.spelling.running = false;
                this.spelling.worker = None;
                if this.spelling.generation == generation
                    && this.editor.read(cx).revision() == revision
                {
                    this.spelling.loading = false;
                    let diagnostics = found
                        .into_iter()
                        .map(|range| Diagnostic { range })
                        .collect();
                    this.editor
                        .update(cx, |editor, cx| editor.set_diagnostics(diagnostics, cx));
                }
                // A newer request whose debounce elapsed gets the sole slot.
                this.start_spellcheck(cx);
                cx.notify();
            });
        }));
    }

    /// Catch text loads and Settings changes once; ordinary edits schedule their
    /// own debounce. The key includes PII exclusions independently of text.
    pub(crate) fn ensure_spellcheck(&mut self, cx: &mut Context<Self>) {
        if self.spelling.scheduled != Some(self.spellcheck_key(cx)) {
            self.schedule_spellcheck(false, cx);
        }
    }

    pub(crate) fn toggle_document_spelling(
        &mut self,
        _: &ToggleDocumentSpelling,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spelling.off_here = !self.spelling.off_here;
        self.spelling.menu_open = false;
        self.schedule_spellcheck(false, cx);
        cx.notify();
    }

    pub(crate) fn next_misspelling(
        &mut self,
        _: &NextMisspelling,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_misspelling(false, window, cx);
    }

    pub(crate) fn previous_misspelling(
        &mut self,
        _: &PreviousMisspelling,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_misspelling(true, window, cx);
    }

    /// Select the next (or previous) flagged word after the selection,
    /// wrapping around, and scroll it into view.
    fn step_misspelling(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.spelling.menu_open = false;
        let editor = self.editor.read(cx);
        let selection = editor.selection();
        let ranges: Vec<_> = editor
            .diagnostics()
            .iter()
            .map(|d| d.range.clone())
            .collect();
        let choice = if backwards {
            ranges
                .iter()
                .rev()
                .find(|r| r.start < selection.start)
                .or(ranges.last())
        } else {
            ranges
                .iter()
                .find(|r| r.start >= selection.end && **r != selection)
                .or(ranges.first())
        };
        let Some(range) = choice.cloned() else {
            cx.notify();
            return;
        };
        self.editor
            .update(cx, |editor, cx| editor.set_selection(range.clone(), cx));
        if let Some(top) = self.editor.read(cx).offset_screen_top(range.start) {
            let viewport = self.scroll.bounds();
            if top < viewport.top() + px(24.) || top > viewport.bottom() - px(48.) {
                let delta = top - viewport.top() - viewport.size.height / 3.;
                let mut scroll = self.scroll.offset();
                scroll.y = (scroll.y - delta).clamp(-self.scroll.max_offset().y, px(0.));
                self.scroll.set_offset(scroll);
            }
        }
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn spelling_control_off(&self) -> bool {
        configured_options(&self.preferences).is_none()
    }
}

#[cfg(test)]
mod scheduling_tests {
    use super::*;
    use crate::document_view_tests::boot;

    #[gpui::test]
    fn typing_captures_only_one_snapshot_after_the_debounce(cx: &mut gpui::TestAppContext) {
        let (app, cx) = boot(cx);
        let before = app.read_with(cx, |app, _| app.spelling.snapshots);
        for word in ["The ", "parties ", "agree "] {
            cx.simulate_input(word);
        }
        assert_eq!(app.read_with(cx, |app, _| app.spelling.snapshots), before);
        cx.executor().advance_clock(EDIT_DELAY);
        cx.run_until_parked();
        app.read_with(cx, |app, cx| {
            assert_eq!(app.spelling.snapshots, before + 1);
            assert_eq!(app.editor.read(cx).text(), "The parties agree ");
            assert!(app.editor.read(cx).diagnostics().is_empty());
        });
    }

    #[gpui::test]
    fn a_running_check_keeps_one_latest_request_and_rejects_stale_results(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = boot(cx);
        let before = app.read_with(cx, |app, _| app.spelling.snapshots);
        app.update(cx, |app, cx| {
            app.editor
                .update(cx, |editor, cx| editor.set_text("partys", cx));
            app.schedule_spellcheck(false, cx);
            app.start_spellcheck(cx);
            assert!(app.spelling.running);
            for text in ["agre", "The parties agree."] {
                app.editor
                    .update(cx, |editor, cx| editor.set_text(text, cx));
                app.schedule_spellcheck(false, cx);
                app.start_spellcheck(cx);
            }
            assert_eq!(app.spelling.snapshots, before + 1);
        });
        cx.run_until_parked();
        app.read_with(cx, |app, cx| {
            assert_eq!(
                app.spelling.snapshots,
                before + 2,
                "only the final waiting request runs"
            );
            assert!(!app.spelling.running);
            assert!(app.editor.read(cx).diagnostics().is_empty());
        });
    }

    #[gpui::test]
    fn theme_refresh_does_not_schedule_spelling_or_case_saves(cx: &mut gpui::TestAppContext) {
        let (app, cx) = boot(cx);
        app.update(cx, |app, cx| {
            let case = cases::Case::new("Fixture".into(), &[], None);
            app.case = Some(cases::CaseLink::of(&case));
            let key = app.spelling.scheduled;
            let snapshots = app.spelling.snapshots;
            app.theme.set(Theme::Light);
            app.sync_pii_theme(cx);
            assert_eq!(app.spelling.scheduled, key);
            assert_eq!(app.spelling.snapshots, snapshots);
            assert!(app.case_save.is_none());
        });
    }

    #[gpui::test]
    fn entity_exclusions_refresh_without_editing_source_and_disable_rejects_work(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = boot(cx);
        app.update(cx, |app, cx| {
            app.editor.update(cx, |e, cx| e.set_text("partys", cx));
        });
        cx.run_until_parked();
        app.read_with(cx, |app, cx| {
            assert_eq!(app.editor.read(cx).diagnostics().len(), 1)
        });
        app.update(cx, |app, cx| {
            app.pii
                .review
                .add_manual("partys", 0..6, pii::Category::Person)
                .unwrap();
            app.review_changed(cx);
        });
        cx.run_until_parked();
        app.read_with(cx, |app, cx| {
            assert!(app.editor.read(cx).diagnostics().is_empty())
        });
        app.update(cx, |app, cx| {
            app.editor.update(cx, |e, cx| e.set_text("agre", cx));
            app.schedule_spellcheck(false, cx);
            app.start_spellcheck(cx);
            app.spelling.off_here = true;
            app.schedule_spellcheck(false, cx);
        });
        cx.run_until_parked();
        app.read_with(cx, |app, cx| {
            assert!(!app.spelling.running);
            assert!(app.editor.read(cx).diagnostics().is_empty());
        });
    }
}
