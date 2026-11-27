//! What Settings does when asked: opening and closing, moving between
//! pages, and saving each change in order.
use super::{Category, Event, Panel, text};
use crate::app::updater;
use gpui::{Context, Window, px};
use mdoc_import::model_work;
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::settings::{Model, Preferences, is_latest_write, next_write, save_in_order};
use std::time::Duration;

impl Panel {
    /// A switch's click: its choice flips in the draft, which saves.
    pub(super) fn flip_switch(&mut self, flip: fn(&mut Preferences), cx: &mut Context<Self>) {
        flip(&mut self.draft);
        self.persist(cx);
    }

    /// A Resolution choice, saved at once.
    pub(super) fn set_dpi(&mut self, dpi: u16, cx: &mut Context<Self>) {
        self.draft.ocr.dpi = dpi;
        self.persist(cx);
    }

    /// An Appearance choice: the window owns and saves it, and it wins over
    /// a General reset still being saved.
    pub(super) fn choose_theme(&mut self, choice: ThemeChoice, cx: &mut Context<Self>) {
        self.theme_revision += 1;
        self.reset_theme = None;
        cx.emit(Event::ThemeSelected(choice))
    }

    /// A Settings › General choice changed and saved outside the panel (the
    /// menu bar shelf's notes): the same change is made in the
    /// draft, keeping its other choices, so the next save keeps it; a save
    /// in flight, which captured the draft before it, is followed by one
    /// with it.
    pub fn general_changed(
        &mut self,
        change: impl FnOnce(&mut mdoc_store::settings::GeneralConfig),
        cx: &mut Context<Self>,
    ) {
        change(&mut self.draft.general);
        if self.applying {
            self.save_pending = true;
        }
        cx.notify();
    }

    /// The window remembered a choice of the AI connector's, such as
    /// Ask's: the draft keeps it, so applying Settings never undoes it.
    pub fn ai_connector_changed(
        &mut self,
        change: impl FnOnce(&mut mdoc_store::settings::AiConnectorConfig),
        cx: &mut Context<Self>,
    ) {
        change(&mut self.draft.ai_connector);
        if self.applying {
            self.save_pending = true;
        }
        cx.notify();
    }

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        if !self.applying {
            self.draft = self.shared.borrow().current.clone().unwrap_or_default();
        }
        self.category = Category::General;
        self.dictionary = None;
        self.recovery.forget();
        self.ai.forget(cx);
        self.number_errors = [None, None];
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        self.error = self
            .save_error
            .clone()
            .or_else(|| self.shared.borrow().error.as_ref().map(ToString::to_string));
        self.update_fields(cx);
        self.previous = window.focused(cx);
        self.open = true;
        window.focus(&self.category_focus, cx);
        self.refresh(cx);
        self.other_work = model_work::description();
        self.refresh_watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let Ok(open) = this.update(cx, |this, cx| {
                    if this.check_pending && !model_work::busy() {
                        this.refresh(cx);
                    }
                    let current = model_work::description();
                    if current != this.other_work {
                        this.other_work = current;
                        cx.notify();
                    }
                    this.open
                }) else {
                    break;
                };
                if !open {
                    break;
                }
            }
        }));
        cx.notify();
    }

    pub(crate) fn sync_theme(&mut self, choice: ThemeChoice, cx: &mut Context<Self>) {
        // A newer explicit choice wins over a General reset still being saved.
        if choice != self.theme_choice {
            self.theme_revision += 1;
            self.reset_theme = None;
        }
        self.theme_choice = choice;
        // The resolved theme may change even while System stays selected.
        cx.notify();
    }

    /// Open Settings at one feature's models, e.g. from a setup card.
    pub fn show_section(&mut self, model: Model, window: &mut Window, cx: &mut Context<Self>) {
        self.show_category(
            match model {
                Model::Ocr(_) => Category::Recognition,
                Model::Pii(_) => Category::Detection,
            },
            window,
            cx,
        );
    }

    /// The update card opens General, which owns Automatic updates.
    pub fn show_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_category(Category::General, window, cx);
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_watch = None;
        self.open = false;
        self.dictionary = None;
        self.recovery.forget();
        self.ai.forget(cx);
        self.commit_visible_number(cx);
        if let Some(focus) = self.previous.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn update_fields(&mut self, cx: &mut Context<Self>) {
        self.threshold.update(cx, |i, cx| {
            i.set_value(self.draft.pseudonymization.threshold.to_string(), cx)
        });
        self.confidence.update(cx, |i, cx| {
            i.set_value(self.draft.ocr.minimum_confidence.to_string(), cx)
        });
    }

    /// Commit just this category's numeric field, preserving the last good value.
    pub(super) fn commit_visible_number(&mut self, cx: &mut Context<Self>) {
        match self.category {
            Category::Detection => self.commit_number(0, cx),
            Category::Recognition => self.commit_number(1, cx),
            _ => {}
        }
    }

    pub(super) fn commit_number(&mut self, index: usize, cx: &mut Context<Self>) {
        let input = if index == 0 {
            &self.threshold
        } else {
            &self.confidence
        };
        let value = input
            .read(cx)
            .value()
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v));
        let Some(value) = value else {
            self.number_errors[index] = Some(text::NUMBER_OUT_OF_RANGE.into());
            cx.notify();
            return;
        };
        self.number_errors[index] = None;
        let previous = if index == 0 {
            self.draft.pseudonymization.threshold
        } else {
            self.draft.ocr.minimum_confidence
        };
        if previous != value {
            if index == 0 {
                self.draft.pseudonymization.threshold = value;
            } else {
                self.draft.ocr.minimum_confidence = value;
            }
            self.persist(cx);
        }
        cx.notify();
    }

    /// Reset only preferences owned by the visible category. About has no preferences.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        let defaults = Preferences::default();
        match self.category {
            Category::General => {
                self.draft.updates = defaults.updates;
                // Notes already seen stay seen.
                self.draft.general.menu_bar = defaults.general.menu_bar;
            }
            Category::Detection => {
                self.draft.pseudonymization = defaults.pseudonymization;
                self.draft.categories = defaults.categories;
                self.threshold.update(cx, |i, cx| {
                    i.set_value(self.draft.pseudonymization.threshold.to_string(), cx)
                });
                self.number_errors[0] = None;
            }
            Category::Recognition => {
                self.draft.ocr = defaults.ocr;
                self.confidence.update(cx, |i, cx| {
                    i.set_value(self.draft.ocr.minimum_confidence.to_string(), cx)
                });
                self.number_errors[1] = None;
            }
            Category::Spelling => self.draft.spelling = defaults.spelling,
            Category::About | Category::Vault | Category::AiConnector => return,
        }
        if self.category == Category::General {
            self.reset_theme = Some(self.theme_revision);
        }
        self.persist(cx);
    }

    /// "AI connector (preview)" as last saved.
    fn ai_connector_saved_on(&self) -> bool {
        (self.shared.borrow().current.as_ref()).is_some_and(|saved| saved.ai_connector.preview)
    }

    /// Atomically save `draft`. A failure restores the previous choices.
    pub(super) fn persist(&mut self, cx: &mut Context<Self>) {
        // The AI connector switch shows or hides Claude Desktop's state at once.
        if self.category == Category::AiConnector {
            self.refresh_ai(cx);
        }
        // Shown off, it serves nothing more, before the save lands (#1220).
        if !self.draft.ai_connector.preview && self.ai_connector_saved_on() {
            cx.emit(Event::AiConnectorShown(false));
        }
        if self.applying {
            self.save_pending = true;
            return;
        }
        let prefs = self.draft.clone();
        let reset_theme = self.reset_theme.take();
        let saved_updates = prefs.updates;
        let check_updates = self
            .shared
            .borrow()
            .current
            .as_ref()
            .is_some_and(|old| old.updates != prefs.updates)
            && prefs.updates.check_automatically;
        let unticked: std::collections::HashSet<_> = self
            .shared
            .borrow()
            .current
            .as_ref()
            .map(|old| {
                mdoc_pii::Category::ALL
                    .into_iter()
                    .filter(|&c| old.categories.detects(c) && !prefs.categories.detects(c))
                    .collect()
            })
            .unwrap_or_default();
        let path = self.shared.borrow().path.clone();
        // Taken now: a later write (a shelf note's) skips this one if it
        // finishes last.
        let write = next_write();
        self.applying = true;
        self.saving_preview = Some(prefs.ai_connector.preview);
        #[cfg(test)]
        let save_gate = self.save_gate.take();
        let task = cx.background_executor().spawn({
            let prefs = prefs.clone();
            async move {
                #[cfg(test)]
                if let Some(gate) = save_gate {
                    gate.await;
                }
                // `Ok(false)`: a later write (a shelf note's) started, and
                // this one's choices are not the latest.
                match path {
                    Some(path) => save_in_order(&path, &prefs, write),
                    None => Ok(is_latest_write(write)),
                }
            }
        });
        cx.spawn(async move |this, cx| {
            // A later write (a shelf note's) may have landed since this one
            // finished: then it is not the latest and publishes nothing.
            let result = task.await.map(|saved| saved && is_latest_write(write));
            let _ = this.update(cx, |this, cx| {
                this.applying = false;
                this.saving_preview = None;
                match result {
                    // Overtaken: publish nothing; the draft, which holds the
                    // later change too, saves next, carrying a General
                    // reset's theme change (a newer theme choice still wins).
                    Ok(false) => {
                        this.save_pending = false;
                        if this.reset_theme.is_none() {
                            this.reset_theme = reset_theme;
                        }
                        this.persist(cx);
                    }
                    Ok(true) => {
                        this.shared.borrow_mut().current = Some(prefs);
                        this.shared.borrow_mut().error = None;
                        this.error = None;
                        this.save_error = None;
                        if reset_theme == Some(this.theme_revision) {
                            cx.emit(Event::ThemeSelected(ThemeChoice::System));
                        }
                        if check_updates && this.draft.updates == saved_updates {
                            updater::spawn_check(saved_updates.include_prereleases, true, cx);
                        }
                        cx.emit(Event::Applied);
                        if !unticked.is_empty() {
                            cx.emit(Event::CategoriesUnticked(unticked));
                        }
                        if this.save_pending {
                            this.save_pending = false;
                            this.persist(cx);
                        }
                    }
                    Err(e) => {
                        this.reset_theme = None;
                        this.save_pending = false;
                        this.draft = this.shared.borrow().current.clone().unwrap_or_default();
                        this.update_fields(cx);
                        if this.category == Category::AiConnector {
                            this.refresh_ai(cx);
                        }
                        // An off that didn't save is on again.
                        if this.ai_connector_saved_on() {
                            cx.emit(Event::AiConnectorShown(true));
                        }
                        this.error = Some(text::not_saved(e));
                        this.save_error = this.error.clone();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn show_category(
        &mut self,
        category: Category,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show(window, cx);
        self.change_category(category, window, cx);
    }

    pub(super) fn change_category(
        &mut self,
        category: Category,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.category != category {
            self.commit_visible_number(cx);
        }
        self.category = category;
        self.dictionary = None;
        self.recovery.forget();
        self.ai.forget(cx);
        if category == Category::AiConnector {
            self.refresh_ai(cx);
        }
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        window.focus(&self.category_focus, cx);
        cx.notify();
    }

    pub(super) fn step_category(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = Category::ALL
            .iter()
            .position(|c| *c == self.category)
            .unwrap();
        let len = Category::ALL.len();
        let next = (index + if backwards { len - 1 } else { 1 }) % len;
        self.change_category(Category::ALL[next], window, cx);
    }

    /// Tick or untick a category to detect and save; a failed save restores
    /// the previous list.
    pub(crate) fn set_detects(
        &mut self,
        category: mdoc_pii::Category,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        self.draft.categories.set(category, on);
        self.persist(cx);
    }
}
