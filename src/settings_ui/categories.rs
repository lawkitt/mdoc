//! Settings presentation and category navigation (ADR 0076).
use super::{
    CloseSettings, CommitNumber, Event, NextCategory, NextSettingsField, Panel, PreviousCategory,
    PreviousSettingsField, block_document_commands, quiet_control,
};
use crate::{
    style::{self, Theme},
    ui,
};
use gpui::{AnyElement, App, Context, Focusable, MouseButton, Window, div, prelude::*, px};
use mdoc_store::settings::{Model, OcrModel, PiiModel, Preferences};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Category {
    #[default]
    General,
    Detection,
    Recognition,
    Spelling,
    Cases,
    About,
}
impl Category {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Detection,
        Self::Recognition,
        Self::Spelling,
        Self::Cases,
        Self::About,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Detection => "Detection",
            Self::Recognition => "Recognition",
            Self::Spelling => "Spelling",
            Self::Cases => "Cases",
            Self::About => "About",
        }
    }
}

/// Height of the fade where a page's rows continue past the viewport.
const SCROLL_FADE: f32 = 20.;

impl Panel {
    pub(crate) fn category(&self) -> Category {
        self.category
    }
    /// Whether rows continue above and below the page's viewport, from its
    /// last layout. The scroll thumb redraws when that layout changes.
    pub(crate) fn scroll_cues(&self) -> (bool, bool) {
        let max = self.scroll.max_offset().y;
        let offset = -self.scroll.offset().y;
        (offset > px(0.5), max > px(1.) && offset < max - px(0.5))
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
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        window.focus(&self.category_focus, cx);
        cx.notify();
    }
    fn step_category(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        let index = Category::ALL
            .iter()
            .position(|c| *c == self.category)
            .unwrap();
        let next = (index + if backwards { 5 } else { 1 }) % Category::ALL.len();
        self.change_category(Category::ALL[next], window, cx);
    }
    pub(super) fn setting_row(
        &self,
        title: &'static str,
        description: impl Into<gpui::SharedString>,
        control: impl IntoElement,
        theme: Theme,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_3()
            .py_3()
            .border_b_1()
            .border_color(theme.pdf_style().border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_size(px(13.)).child(title))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme.pdf_style().header_muted)
                            .child(description.into()),
                    ),
            )
            .child(div().flex_shrink_0().child(control))
            .into_any_element()
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn switch(
        &self,
        id: &'static str,
        label: &'static str,
        on: bool,
        enabled: bool,
        flip: fn(&mut Preferences),
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // On is the accent; off is neutral grey, so an off switch never
        // reads as a tinted on one (ADR 0076).
        let accent = theme.search_accent();
        let p = theme.pdf_style();
        let off = p.header_muted;
        self.scrolled_quiet(id, "", theme, enabled, cx)
            .when(cfg!(test), |v| v.debug_selector(move || id.into()))
            .role(gpui::Role::Switch)
            .aria_label(label)
            .aria_toggled(if on {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .w(px(36.))
            .h(px(22.))
            .p(px(3.))
            .rounded_full()
            .flex()
            .items_center()
            .border_1()
            .border_color(if on { accent } else { off })
            .bg(if on { accent } else { p.placeholder_bg })
            .when(on, |v| v.justify_end())
            .child(
                div()
                    .w(px(14.))
                    .h(px(14.))
                    .rounded_full()
                    .bg(if on { p.bg } else { off }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if enabled {
                    flip(&mut this.draft);
                    this.persist(cx);
                }
            }))
            .into_any_element()
    }
    /// One choice of a segmented control (Appearance, Resolution). The
    /// selected choice is filled with the accent, so it never reads as the
    /// focused one, which only gains the outside ring (ADR 0076). The caller
    /// adds the label.
    pub(super) fn segment(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<gpui::SharedString>,
        selected: bool,
        theme: Theme,
        cx: &App,
    ) -> ui::Control {
        let p = theme.pdf_style();
        let accent = theme.search_accent();
        let id = id.into();
        let segment = ui::focus_ring(id.clone(), theme)
            .role(gpui::Role::RadioButton)
            .aria_label(label)
            .aria_toggled(if selected {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .key_context("UiControl")
            .tab_index(0)
            .tab_stop(true)
            .flex_shrink_0()
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .text_size(px(13.))
            .cursor_pointer()
            .map(|v| {
                if selected {
                    v.bg(accent)
                        .border_color(accent)
                        .text_color(p.bg)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                } else {
                    v.border_color(p.border)
                        .text_color(p.header_fg)
                        .hover(move |s| s.bg(p.placeholder_bg))
                }
            });
        self.reveal(id, segment, true, cx)
    }
    fn spelling_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.draft.spelling;
        let mut rows = div().flex().flex_col();
        for (id, title, description, on, enabled, flip) in [
            (
                "spelling-enabled",
                "Check spelling",
                "Flags likely typos and recognition errors. Code, links and detected names are skipped.",
                s.enabled,
                true,
                (|p: &mut Preferences| p.spelling.enabled = !p.spelling.enabled)
                    as fn(&mut Preferences),
            ),
            (
                "spelling-english",
                "English",
                "Check words against the bundled English dictionary.",
                s.english,
                s.enabled,
                (|p: &mut Preferences| p.spelling.english = !p.spelling.english)
                    as fn(&mut Preferences),
            ),
            (
                "spelling-russian",
                "Russian",
                "Check words against the bundled Russian dictionary.",
                s.russian,
                s.enabled,
                (|p: &mut Preferences| p.spelling.russian = !p.spelling.russian)
                    as fn(&mut Preferences),
            ),
            (
                "spelling-caps",
                "Check ALL-CAPS words",
                "Include words written entirely in capitals.",
                s.check_all_caps,
                s.enabled,
                (|p: &mut Preferences| p.spelling.check_all_caps = !p.spelling.check_all_caps)
                    as fn(&mut Preferences),
            ),
            (
                "spelling-digits",
                "Check words with digits",
                "Include words that contain numbers.",
                s.check_digits,
                s.enabled,
                (|p: &mut Preferences| p.spelling.check_digits = !p.spelling.check_digits)
                    as fn(&mut Preferences),
            ),
        ] {
            rows = rows.child(self.setting_row(
                title,
                description,
                self.switch(id, title, on, enabled, flip, theme, cx),
                theme,
            ));
        }
        rows.child(self.dictionary_summary(theme, cx))
            .into_any_element()
    }
    /// Tick or untick a category to detect and save; a failed save restores
    /// the previous list (ADR 0076).
    pub(crate) fn set_detects(
        &mut self,
        category: mdoc_pii::Category,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        self.draft.categories.set(category, on);
        self.persist(cx);
    }
    /// Categories to detect (ADR 0076): one chip per category scans can
    /// return, all on by default. Saves on change.
    fn categories_row(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        div()
            .id("settings-detect-categories")
            .flex()
            .flex_col()
            .gap_2()
            .py_3()
            .border_b_1()
            .border_color(p.border)
            .child(div().text_size(px(13.)).child("Categories to detect"))
            .child(div().text_size(px(11.)).text_color(p.header_muted).child(
                "Scans of documents without a case skip categories that are off, and new \
                 cases start with this list. Names you mark yourself are still replaced.",
            ))
            .child(
                div()
                    .id("settings-detect-chips")
                    .role(gpui::Role::Group)
                    .aria_label("Categories to detect")
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(mdoc_pii::Category::DETECTED.into_iter().map(|category| {
                        let on = self.draft.categories.detects(category);
                        let id = format!("settings-detect-{}", category.token());
                        self.scrolled_control(
                            gpui::SharedString::from(id.clone()),
                            format!(
                                "{} {}",
                                if on { "✓" } else { "○" },
                                category.detection_label()
                            ),
                            theme,
                            true,
                            cx,
                        )
                        .when(cfg!(test), move |v| v.debug_selector(move || id.clone()))
                        .role(gpui::Role::CheckBox)
                        .aria_label(category.detection_label())
                        .aria_toggled(if on {
                            gpui::Toggled::True
                        } else {
                            gpui::Toggled::False
                        })
                        .when(on, |v| {
                            v.bg(theme.sidebar_selected())
                                .border_color(theme.search_accent())
                        })
                        .when(!on, |v| v.text_color(p.header_muted))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.set_detects(category, !on, cx)),
                        )
                    })),
            )
            .into_any_element()
    }
    fn number_row(&self, index: usize, theme: Theme, cx: &App) -> AnyElement {
        let (id, title, description, input) = if index == 0 {
            (
                "settings-threshold-field",
                "Detection threshold",
                "From 0 to 1. Higher values return fewer candidates.",
                &self.threshold,
            )
        } else {
            (
                "settings-confidence-field",
                "Minimum confidence",
                "From 0 to 1. Confidence is not measured accuracy.",
                &self.confidence,
            )
        };
        div()
            .flex()
            .flex_col()
            .child(
                self.setting_row(
                    title,
                    description,
                    div()
                        .id(id)
                        .key_context("SettingsNumber")
                        .w(px(80.))
                        .border_1()
                        .rounded_md()
                        .border_color(theme.pdf_style().border)
                        .px_2()
                        .py_1()
                        .child(input.clone())
                        .map(|v| {
                            ui::reveal_focus(
                                v,
                                input.read(cx).focus_handle(cx),
                                self.scroll.clone(),
                            )
                        }),
                    theme,
                ),
            )
            .children(self.number_errors[index].clone().map(|error| {
                div()
                    .text_size(px(11.))
                    .text_color(style::markdown_style(theme).alert_warning)
                    .child(error)
            }))
            .into_any_element()
    }
    fn about_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div().flex().flex_col().child(self.setting_row(
            "mdoc",
            format!(
                "Version {} · {}",
                env!("CARGO_PKG_VERSION"),
                mdoc_store::cases::build_identity()
            ),
            div(),
            theme,
        ));
        for (id, title, description, url) in [
            (
                "about-help",
                "Help",
                "Learn how to prepare and review documents.",
                crate::menus::HELP_URL,
            ),
            (
                "about-website",
                "Website",
                "Visit the mdoc project.",
                "https://github.com/lawkitt/mdoc",
            ),
            (
                "about-issue",
                "Report an issue",
                "Describe a problem or suggest an improvement.",
                "https://github.com/lawkitt/mdoc/issues/new",
            ),
            (
                "about-licence",
                "App license",
                "GNU General Public License, version 3 or later.",
                "https://github.com/lawkitt/mdoc/blob/main/LICENSE",
            ),
        ] {
            rows = rows.child(
                self.setting_row(
                    title,
                    description,
                    self.scrolled_quiet(id, "Open ↗", theme, true, cx)
                        .aria_label(title)
                        .on_click(move |_, _, cx| cx.open_url(url)),
                    theme,
                ),
            );
        }
        rows.child(div().py_3().text_size(px(11.)).text_color(theme.pdf_style().header_muted).child("Your documents stay on this computer. Processing is local; no document text is sent to an AI service. Update checks send only the app version. Models and updates download only on request."))
            .into_any_element()
    }
    fn category_body(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let body = match self.category() {
            Category::General => div().child(self.appearance_section(theme, cx))
                // ADR 0084: macOS only; Windows has no tray yet.
                .when(cfg!(target_os = "macos"), |v| v.child(self.setting_row("Show mdoc in the menu bar", "mdoc keeps running after you close its windows.", self.switch("general-menu-bar", "Show mdoc in the menu bar", self.draft.general.menu_bar, true, |p| p.general.menu_bar = !p.general.menu_bar, theme, cx), theme)))
                .child(self.updates_section(theme, cx))
                .child(self.setting_row("Recent files", "Names and folders of recently opened files, never their contents.", self.scrolled_quiet("recents-clear", "Clear", theme, true, cx).when(cfg!(test), |v| v.debug_selector(|| "recents-clear".into())).on_click(cx.listener(|_, _, _, cx| cx.emit(Event::ClearRecents))), theme))
                .child(self.setting_row("Keyboard shortcuts", "Browse the keys available in each part of mdoc.", self.scrolled_quiet("settings-shortcuts", "Show", theme, true, cx).when(cfg!(test), |v| v.debug_selector(|| "settings-shortcuts".into())).aria_label("Keyboard shortcuts").on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Shortcuts))), theme)),
            Category::Detection => div()
                .child(div().text_size(px(11.)).text_color(theme.pdf_style().header_muted).child(self.section_note("Detection can miss names and identifiers — review the whole document before sharing.", Model::Pii(self.draft.pseudonymization.model))))
                .child(div().id("pii-models").role(gpui::Role::RadioGroup).aria_label("Pseudonymization").children(PiiModel::ALL.into_iter().map(|m| self.row(Model::Pii(m), cx))))
                .child(self.number_row(0, theme, cx))
                .child(self.categories_row(theme, cx))
                .child(self.user_lists_section(theme, cx))
                .child(self.setting_row("Compare pseudonymization", "Compare models on a snapshot without changing your document or defaults.", self.scrolled_quiet("compare-pii", "Compare", theme, true, cx).aria_label("Compare pseudonymization").on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Compare(Model::Pii(PiiModel::Fp16))))), theme)),
            Category::Recognition => {
                let note = self.section_note("Default when a document suggests no language. Each document’s card picks its own.", Model::Ocr(self.draft.ocr.model));
                div().child(div().text_size(px(11.)).text_color(theme.pdf_style().header_muted).child(note.clone()).when(cfg!(test), |v| v.debug_selector(move || note.clone())))
                    .child(div().id("ocr-models").role(gpui::Role::RadioGroup).aria_label("Text recognition").children(OcrModel::ALL.into_iter().map(|m| self.row(Model::Ocr(m), cx))))
                    .child(self.setting_row("Resolution", "Pixels per inch for recognized pages.", div().id("settings-dpi").role(gpui::Role::RadioGroup).aria_label("Resolution").flex().gap_2().children([150, 200, 300].map(|dpi| self.segment(("dpi", dpi as usize), format!("{dpi} DPI"), self.draft.ocr.dpi == dpi, theme, cx)
                        .child(format!("{dpi}"))
                        .when(cfg!(test), |v| v.debug_selector(move || format!("settings-dpi-{dpi}")))
                        .on_click(cx.listener(move |this, _, _, cx| { this.draft.ocr.dpi = dpi; this.persist(cx); })))), theme))
                    .child(self.number_row(1, theme, cx))
                    .child(self.setting_row("Compare text recognition", "Compare models on a snapshot without changing your document or defaults.", self.scrolled_quiet("compare-ocr", "Compare", theme, true, cx).aria_label("Compare text recognition").on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Compare(Model::Ocr(OcrModel::Cyrillic))))), theme))
            },
            Category::Spelling => div().child(self.spelling_section(theme, cx)),
            Category::Cases => div().child(self.setting_row("Ask before closing", "Confirm before closing an unsaved mapping. A saved case keeps its mapping encrypted on this computer.", self.switch("cases-ask", "Ask before closing an unsaved mapping", self.draft.cases.ask_before_closing, true, |p| p.cases.ask_before_closing = !p.cases.ask_before_closing, theme, cx), theme)),
            Category::About => div().child(self.about_section(theme, cx)),
        };
        // About has no preferences, so it has nothing to reset (ADR 0076).
        body.when(self.category != Category::About, |v| {
            v.child(
                self.setting_row(
                    "Reset",
                    "Restore this category's defaults.",
                    self.scrolled_quiet("reset-settings", "Reset", theme, true, cx)
                        .when(cfg!(test), |v| v.debug_selector(|| "reset-settings".into()))
                        .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
                    theme,
                ),
            )
        })
        .into_any_element()
    }
    pub(super) fn category_render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        ))]
        self.appearance_bounds.set(None);
        if self.number_subscriptions.is_empty() {
            for (index, input) in [&self.threshold, &self.confidence].into_iter().enumerate() {
                let focus = input.read(cx).focus_handle(cx);
                self.number_subscriptions
                    .push(cx.on_blur(&focus, window, move |this, _, cx| {
                        this.commit_number(index, cx)
                    }));
            }
        }
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let categories = div()
            .id("settings-categories")
            .role(gpui::Role::TabList)
            .aria_label("Settings categories")
            .aria_orientation(gpui::accesskit::Orientation::Vertical)
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .w(px(if window.viewport_size().width < px(640.) {
                140.
            } else {
                180.
            }))
            .flex_shrink_0()
            .border_r_1()
            .border_color(p.border)
            .children(Category::ALL.into_iter().map(|category| {
                let selected = category == self.category;
                ui::focus_ring(("settings-category", category as usize), theme)
                    .role(gpui::Role::Tab)
                    .aria_label(category.label())
                    .aria_selected(selected)
                    .key_context("SettingsCategories UiControl")
                    .tab_stop(selected)
                    .when(selected, |v| v.track_focus(&self.category_focus))
                    .when(cfg!(test), |v| {
                        v.debug_selector(move || format!("settings-category-{}", category.label()))
                    })
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .child(category.label())
                    .when(selected, |v| v.bg(theme.sidebar_selected()))
                    .hover(|v| v.bg(p.placeholder_bg))
                    .on_action(cx.listener(|this, _: &NextCategory, window, cx| {
                        this.step_category(false, window, cx);
                        cx.stop_propagation();
                    }))
                    .on_action(cx.listener(|this, _: &PreviousCategory, window, cx| {
                        this.step_category(true, window, cx);
                        cx.stop_propagation();
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.change_category(category, window, cx)
                    }))
            }));
        let content = div()
            .id("settings-content")
            .role(gpui::Role::TabPanel)
            .aria_label(self.category.label())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_4()
                    .pt_3()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .when(self.dictionary.is_some(), |v| {
                        v.child(
                            quiet_control("close-dictionary", "← Spelling", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "close-dictionary".into())
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_dictionary(window, cx)
                                })),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(if self.dictionary.is_some() {
                                "Your dictionary"
                            } else {
                                self.category.label()
                            }),
                    ),
            )
            .when(self.dictionary.is_some(), |v| {
                v.child(self.dictionary_body(theme, cx))
            })
            .when(self.dictionary.is_none(), |v| {
                // A page taller than the overlay shows a scroll thumb, and
                // fades at an edge where rows continue (ADR 0076).
                let (above, below) = self.scroll_cues();
                let fade = |top: bool| {
                    let clear = gpui::Hsla { a: 0., ..p.bg };
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .h(px(SCROLL_FADE))
                        .when(top, |v| v.top_0())
                        .when(!top, |v| v.bottom_0())
                        .bg(gpui::linear_gradient(
                            if top { 0. } else { 180. },
                            gpui::linear_color_stop(clear, 0.),
                            gpui::linear_color_stop(p.bg, 1.),
                        ))
                };
                v.child(
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .min_h_0()
                        .flex_1()
                        .child(
                            div()
                                .id("settings-scroll")
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "settings-scroll".into())
                                })
                                .track_scroll(&self.scroll)
                                .overflow_y_scroll()
                                .min_h_0()
                                .flex_1()
                                .px_4()
                                .pb_3()
                                .child(self.category_body(theme, cx)),
                        )
                        .when(above, |v| {
                            v.child(fade(true).when(cfg!(test), |v| {
                                v.debug_selector(|| "settings-fade-top".into())
                            }))
                        })
                        .when(below, |v| {
                            v.child(fade(false).when(cfg!(test), |v| {
                                v.debug_selector(|| "settings-fade-bottom".into())
                            }))
                        })
                        .child(gpui_pdf::scrollbar::overlay_scrollbar(
                            "settings-scrollbar",
                            &self.scroll,
                            false,
                            p.header_muted,
                            cx,
                        )),
                )
            })
            .when_some(self.error.clone(), |v, e| {
                v.child(
                    div()
                        .id("settings-error")
                        .when(cfg!(test), |v| v.debug_selector(|| "settings-error".into()))
                        .px_4()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(style::markdown_style(theme).alert_warning)
                        .child(e),
                )
            })
            .when_some(
                self.other_work
                    .filter(|_| self.working.is_none() && !self.checking),
                |v, work| {
                    v.child(
                        ui::activity("model-work-activity", work, theme)
                            .px_4()
                            .pb_2(),
                    )
                },
            );
        div()
            .id("settings-overlay")
            .absolute()
            .inset_0()
            .p_4()
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .bg(gpui::Hsla { a: 0.35, ..p.bg })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.close(window, cx);
                }),
            )
            .child(
                ui::panel("settings-dialog", theme)
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "settings-dialog".into())
                    })
                    .role(gpui::Role::Dialog)
                    .aria_label("Settings")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .key_context("ModelSettings UiPanel")
                    .track_focus(&self.focus)
                    .tab_group()
                    .tab_stop(false)
                    .w(px(800.))
                    .max_w_full()
                    .h(px(600.))
                    .max_h((window.viewport_size().height - px(32.)).max(px(160.)))
                    .flex()
                    .flex_col()
                    .rounded_lg()
                    .shadow_lg()
                    .bg(p.bg)
                    .text_color(p.header_fg)
                    .text_size(px(13.))
                    .border_1()
                    .border_color(p.border)
                    .on_action(
                        cx.listener(|this, _: &CloseSettings, window, cx| this.close(window, cx)),
                    )
                    .map(block_document_commands)
                    .on_action(
                        cx.listener(|this, _: &CommitNumber, _, cx| this.commit_visible_number(cx)),
                    )
                    .on_action(cx.listener(|this, _: &NextSettingsField, window, cx| {
                        ui::cycle(window, cx, Some(&this.focus), false);
                    }))
                    .on_action(cx.listener(|this, _: &PreviousSettingsField, window, cx| {
                        ui::cycle(window, cx, Some(&this.focus), true);
                    }))
                    .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                        ui::cycle(window, cx, Some(&this.focus), false);
                        cx.stop_propagation();
                    }))
                    .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                        ui::cycle(window, cx, Some(&this.focus), true);
                        cx.stop_propagation();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(p.border)
                            .flex_shrink_0()
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Settings"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .child(categories)
                            .child(content),
                    )
                    // Last in Tab order, positioned in the header's corner.
                    .child(
                        quiet_control("close-settings", "×", theme, true)
                            .aria_label("Close Settings")
                            .absolute()
                            .top(px(4.))
                            .right(px(8.))
                            .when(cfg!(test), |v| v.debug_selector(|| "settings-close".into()))
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    ),
            )
            .into_any_element()
    }
}
