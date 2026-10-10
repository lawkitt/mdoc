//! Spellcheck in the document view (ADR 0036): background checks with a
//! per-document line cache, the session-only per-document switch, the toolbar
//! indicator and next/previous navigation. Checking and suggestions never run
//! on the UI thread; it only copies the text and the PII ranges.
use super::*;
use mdoc_editor::Diagnostic;
use mdoc_spell::{Accepted, Exclusions, LineCache, Options, Speller};
use std::{collections::HashSet, ops::Range, sync::Mutex, time::Duration};

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
    /// Where the press that closed the menu landed: a press on the
    /// indicator closes the menu, and its click must not reopen it.
    closed_at: Option<gpui::Point<gpui::Pixels>>,
    /// Words ignored in this document ("Ignore in this document"). Session-only.
    ignored: HashSet<String>,
    ignored_generation: u64,
    /// (editor revision, options, accepted-words generation) of the last
    /// scheduled check.
    scheduled: Option<(u64, Option<Options>, u64)>,
    generation: u64,
    cache: Arc<Mutex<LineCache>>,
    task: Option<gpui::Task<()>>,
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

    /// Check the current text in the background, after [`EDIT_DELAY`] when
    /// `after_edit`. A newer call supersedes a pending one.
    pub(crate) fn schedule_spellcheck(&mut self, after_edit: bool, cx: &mut Context<Self>) {
        let options = self.spelling_options();
        let revision = self.editor.read(cx).revision();
        self.spelling.scheduled = Some((revision, options, self.accepted_generation()));
        self.spelling.generation += 1;
        let Some(options) = options else {
            self.spelling.task = None;
            self.spelling.loading = false;
            if !self.editor.read(cx).diagnostics().is_empty() {
                self.editor
                    .update(cx, |editor, cx| editor.set_diagnostics(Vec::new(), cx));
            }
            return;
        };
        let text = self.editor.read(cx).text().to_owned();
        let review = &self.pii.review;
        // Proposed entities keep only the mixed-script check; applied aliases
        // are never checked.
        let entities: Vec<Range<usize>> = review
            .candidates()
            .iter()
            .map(|o| o.range.clone())
            .collect();
        let aliases: Vec<Range<usize>> = review.applied().iter().map(|o| o.range.clone()).collect();
        self.spelling.loading = !Speller::is_loaded(options);
        let accepted = self.accepted_words();
        let cache = self.spelling.cache.clone();
        let generation = self.spelling.generation;
        self.spelling.task = Some(cx.spawn(async move |this, cx| {
            if after_edit {
                cx.background_executor().timer(EDIT_DELAY).await;
            }
            let found = cx
                .background_executor()
                .spawn(async move {
                    let speller = Speller::load(options).with_accepted(accepted);
                    let mut structural = mdoc_editor::spell_exclusions(&text);
                    structural.extend(aliases);
                    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
                    let exclusions = Exclusions {
                        structural: &structural,
                        entities: &entities,
                    };
                    speller.check_cached(&text, exclusions, &mut cache)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.spelling.generation != generation {
                    return;
                }
                this.spelling.loading = false;
                this.spelling.task = None;
                // The text moved on without a reschedule (a load): render
                // notices the stale revision and checks again.
                if this.editor.read(cx).revision() == revision {
                    let diagnostics = found
                        .into_iter()
                        .map(|range| Diagnostic { range })
                        .collect();
                    this.editor
                        .update(cx, |editor, cx| editor.set_diagnostics(diagnostics, cx));
                }
                cx.notify();
            });
        }));
    }

    /// Catch text loads and Settings changes that bypassed an explicit
    /// schedule. Called from render; schedules at most once per change.
    pub(crate) fn ensure_spellcheck(&mut self, cx: &mut Context<Self>) {
        let key = (
            self.editor.read(cx).revision(),
            self.spelling_options(),
            self.accepted_generation(),
        );
        if self.spelling.scheduled != Some(key) {
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

    #[cfg(test)]
    pub(crate) fn spelling_control_off(&self) -> bool {
        configured_options(&self.preferences).is_none()
    }

    /// Turn spellcheck off (or back on) in Settings, for every document.
    fn set_spelling_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.spelling.menu_open = false;
        let Ok(mut spelling) = self.preferences.borrow().snapshot().map(|p| p.spelling) else {
            return;
        };
        spelling.enabled = enabled;
        // Turning on with no language chosen would still check nothing.
        if enabled && !spelling.english && !spelling.russian {
            spelling.english = true;
            spelling.russian = true;
        }
        self.model_panel
            .update(cx, |panel, cx| panel.set_spelling(spelling, cx));
        cx.notify();
    }

    /// The toolbar control: a spelling icon with a badge counting flagged
    /// words while this document is checked. Its menu switches checking for
    /// this document and turns spellcheck off or on for all documents; while
    /// off, the icon stays visible but faded. Hidden for source-only tabs and
    /// while there is no text yet.
    pub(super) fn spelling_control(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if self.source_only || self.editor.read(cx).text().is_empty() {
            return None;
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let count = self.editor.read(cx).diagnostics().len();
        let on = configured_options(&self.preferences).is_some();
        let checking_here = !self.spelling.off_here;
        let active = on && checking_here;
        let label: gpui::SharedString = if !on {
            "Spelling: off".into()
        } else if !checking_here {
            "Spelling: off in this document".into()
        } else if self.spelling.loading {
            "Spelling: checking…".into()
        } else {
            match count {
                0 => "Spelling: no misspellings".into(),
                1 => "Spelling: 1 misspelling".into(),
                n => format!("Spelling: {n} misspellings").into(),
            }
        };
        let row = |id: &'static str, label: &'static str, enabled: bool| {
            ui::control(id, label, theme, enabled)
                .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
        };
        let menu = ui::panel("spelling-menu", theme)
            .min_w(px(220.))
            .p_1()
            .flex()
            .flex_col()
            .on_mouse_down_out(cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                this.spelling.menu_open = false;
                this.spelling.closed_at = Some(event.position);
                cx.notify();
            }))
            .child(
                row("spelling-document", "Check in this document", on)
                    .aria_toggled(if checking_here {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .child(div().w(px(14.)).children(checking_here.then(|| {
                        gpui::svg()
                            .data(ui::Icon::Check.data())
                            .size(px(14.))
                            .text_color(theme.search_accent())
                    })))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if on {
                            this.toggle_document_spelling(&ToggleDocumentSpelling, window, cx)
                        }
                    })),
            )
            .child(if on {
                row("spelling-off", "Turn off", true)
                    .on_click(cx.listener(|this, _, _, cx| this.set_spelling_enabled(false, cx)))
            } else {
                row("spelling-on", "Turn on", true)
                    .on_click(cx.listener(|this, _, _, cx| this.set_spelling_enabled(true, cx)))
            });
        // The badge counts flagged words; it disappears while loading, at
        // zero, and when this document isn't checked.
        let badge = (active && !self.spelling.loading && count > 0).then(|| {
            div()
                .when(cfg!(test), |v| v.debug_selector(|| "spelling-badge".into()))
                .absolute()
                .top(px(1.))
                .right(px(-2.))
                .h(px(15.))
                .min_w(px(15.))
                .px(px(4.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(style::markdown_style(theme).alert_warning)
                .text_color(p.bg)
                .text_size(px(10.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(if count > 99 {
                    "99+".to_string()
                } else {
                    count.to_string()
                })
        });
        Some(
            div()
                .relative()
                .child(
                    ui::untipped_icon_button(
                        "spelling-indicator",
                        label.clone(),
                        ui::Icon::Spelling,
                        theme,
                        true,
                    )
                    // The tooltip would cover the open menu.
                    .when(!self.spelling.menu_open, |v| {
                        v.tooltip(style::tooltip(label.to_string(), theme))
                    })
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "spelling-indicator".into())
                    })
                    .when(!active, |v| v.opacity(0.5))
                    .on_click(cx.listener(
                        |this, event: &gpui::ClickEvent, _, cx| {
                            let closing_press = match event {
                                gpui::ClickEvent::Mouse(click) => {
                                    this.spelling.closed_at == Some(click.down.position)
                                }
                                _ => false,
                            };
                            this.spelling.closed_at = None;
                            this.spelling.menu_open = !this.spelling.menu_open && !closing_press;
                            cx.notify();
                        },
                    )),
                )
                .children(badge)
                .when(self.spelling.menu_open, |v| {
                    v.child(
                        div().absolute().top_full().left_0().child(gpui::deferred(
                            gpui::anchored()
                                .anchor(gpui::Anchor::TopLeft)
                                .snap_to_window()
                                .child(menu),
                        )),
                    )
                })
                .into_any_element(),
        )
    }
}
