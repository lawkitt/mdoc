//! The Settings dialog as drawn: its frame, the category list, the rows and
//! switches every page uses, and the General, Detection, Recognition,
//! Spelling and About pages.
use super::{
    Category, CloseSettings, CommitNumber, Event, NextCategory, NextSettingsField, Panel,
    PreviousCategory, PreviousSettingsField, text,
};
use crate::{
    app::updater,
    document::{ExportMarkdown, Save},
    ui,
    ui::style::{self, Theme},
    workspace::{Close, Import, New, Open, SwitchDocument, SwitchDocumentBack},
};
use gpui::{AnyElement, App, Context, Focusable, MouseButton, Window, div, prelude::*, px};
use mdoc_store::session_store::ThemeChoice;
use mdoc_store::settings::{Model, OcrModel, PiiModel, Preferences};

/// Keep document commands blocked when another sheet covers Settings too.
pub(crate) fn block_document_commands<E: gpui::InteractiveElement>(element: E) -> E {
    element
        .on_action(|_: &New, _, cx| cx.stop_propagation())
        .on_action(|_: &Open, _, cx| cx.stop_propagation())
        .on_action(|_: &Close, _, cx| cx.stop_propagation())
        .on_action(|_: &SwitchDocument, _, cx| cx.stop_propagation())
        .on_action(|_: &SwitchDocumentBack, _, cx| cx.stop_propagation())
        .on_action(|_: &Save, _, cx| cx.stop_propagation())
        .on_action(|_: &ExportMarkdown, _, cx| cx.stop_propagation())
        .on_action(|_: &Import, _, cx| cx.stop_propagation())
}

pub fn control(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> ui::Control {
    ui::action_control(id, label, theme, enabled, false)
}

pub(super) fn quiet_control(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    theme: Theme,
    enabled: bool,
) -> ui::Control {
    let label = label.into();
    crate::ui::focus_ring(id, theme)
        .key_context("UiControl")
        .tab_index(0)
        .tab_stop(enabled)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .px_2()
        .py_1()
        .rounded_md()
        .text_size(px(12.))
        .text_color(theme.pdf_style().header_muted)
        .child(label)
        .when(enabled, |v| {
            v.cursor_pointer().hover(|v| {
                v.bg(theme.pdf_style().placeholder_bg)
                    .text_color(theme.pdf_style().header_fg)
            })
        })
        .when(!enabled, |v| v.opacity(0.45))
}

/// Height of the fade where a page's rows continue past the viewport.
const SCROLL_FADE: f32 = 20.;

impl Panel {
    pub(super) fn reveal(
        &self,
        id: impl Into<gpui::ElementId>,
        control: ui::Control,
        enabled: bool,
        cx: &App,
    ) -> ui::Control {
        let focus = self
            .controls
            .borrow_mut()
            .entry(id.into())
            .or_insert_with(|| cx.focus_handle())
            .clone()
            .tab_stop(enabled);
        ui::reveal_focus(control.track_focus(&focus), focus, self.scroll.clone())
    }

    pub(super) fn scrolled_control(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<gpui::SharedString>,
        theme: Theme,
        enabled: bool,
        cx: &App,
    ) -> ui::Control {
        let id = id.into();
        self.reveal(id.clone(), control(id, label, theme, enabled), enabled, cx)
    }

    pub(super) fn scrolled_quiet(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<gpui::SharedString>,
        theme: Theme,
        enabled: bool,
        cx: &App,
    ) -> ui::Control {
        let id = id.into();
        self.reveal(
            id.clone(),
            quiet_control(id, label, theme, enabled),
            enabled,
            cx,
        )
    }

    /// Settings → Updates: detection and, on macOS, installation
    /// after an explicit click.
    fn updates_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let p = theme.pdf_style();
        let updates = self.draft.updates;
        let state = updater::state(cx);
        let prerelease_build = updater::is_prerelease_build();
        let installs = !matches!(
            state.eligibility,
            Some(Err(updater::Ineligible::Unsupported))
        );
        let status = match &state.install {
            updater::Install::Downloading { version, .. } => {
                text::downloading_update(version, state.percent().unwrap_or(0))
            }
            updater::Install::Ready { version, .. } => text::update_ready(version),
            updater::Install::Failed { version, message } => text::update_failed(version, message),
            updater::Install::Idle if state.checking => text::CHECKING_FOR_UPDATES.to_string(),
            updater::Install::Idle => match &state.available {
                Some(update) => text::update_available(&update.version, env!("CARGO_PKG_VERSION")),
                None if state.checked => text::latest_version(env!("CARGO_PKG_VERSION")),
                None => text::current_version(env!("CARGO_PKG_VERSION")),
            },
        };
        // Why this copy can't install the available update, if it can't.
        let blocked = state
            .available
            .as_ref()
            .and_then(|_| state.installable().err())
            .filter(|why| !why.is_empty() && installs);
        let can_install = state.installable().is_ok()
            && matches!(
                state.install,
                updater::Install::Idle | updater::Install::Failed { .. }
            );
        let notes = state.available.as_ref().map(|u| {
            let text = u.notes.trim();
            match text.char_indices().nth(400) {
                Some((end, _)) => format!("{}…", &text[..end]),
                None => text.to_owned(),
            }
        });
        let release = state
            .available
            .as_ref()
            .map(|u| u.html_url.clone())
            .or_else(|| match &state.install {
                updater::Install::Failed { version, .. } => Some(updater::release_url(version)),
                _ => None,
            });
        let busy = matches!(state.install, updater::Install::Downloading { .. });
        let actions = div()
            .flex()
            .flex_wrap()
            .justify_end()
            .items_center()
            .gap_2()
            .when(!busy, |v| {
                v.child(
                    self.scrolled_quiet(
                        "updates-check",
                        text::CHECK_NOW,
                        theme,
                        !state.checking,
                        cx,
                    )
                    .when(cfg!(test), |v| v.debug_selector(|| "updates-check".into()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        updater::spawn_check(this.draft.updates.include_prereleases, false, cx)
                    })),
                )
            })
            .when(can_install, |v| {
                v.child(
                    self.scrolled_control(
                        "updates-install",
                        if matches!(state.install, updater::Install::Failed { .. }) {
                            text::TRY_AGAIN
                        } else {
                            text::DOWNLOAD_AND_INSTALL
                        },
                        theme,
                        true,
                        cx,
                    )
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "updates-install".into())
                    })
                    .on_click(|_, _, cx| updater::start_install(cx)),
                )
            })
            .when(busy, |v| {
                v.child(
                    self.scrolled_quiet("updates-cancel", crate::ui::text::CANCEL, theme, true, cx)
                        .when(cfg!(test), |v| v.debug_selector(|| "updates-cancel".into()))
                        .on_click(|_, _, cx| updater::cancel_install(cx)),
                )
            })
            .when(
                matches!(state.install, updater::Install::Ready { .. }),
                |v| {
                    v.child(
                        self.scrolled_control(
                            "updates-restart",
                            text::RESTART_NOW,
                            theme,
                            true,
                            cx,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "updates-restart".into())
                        })
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::RestartToUpdate))),
                    )
                },
            )
            .when_some(release, |v, url| {
                v.child(
                    self.scrolled_control("updates-view", text::VIEW_RELEASE, theme, true, cx)
                        .when(cfg!(test), |v| v.debug_selector(|| "updates-view".into()))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            });
        // The switch has a row of its own, so every Settings switch shares
        // the right-hand column; the status and its actions follow.
        div()
            .flex()
            .flex_col()
            .child(self.setting_row(
                text::AUTOMATIC_UPDATES,
                text::AUTOMATIC_UPDATES_DESCRIPTION,
                self.switch(
                    "updates-automatic",
                    text::CHECK_WHEN_MDOC_STARTS,
                    updates.check_automatically,
                    true,
                    |p| p.updates.check_automatically = !p.updates.check_automatically,
                    theme,
                    cx,
                ),
                theme,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py_3()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .when(cfg!(test), |v| v.debug_selector(|| "updates-status".into()))
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .child(status),
                    )
                    .child(div().max_w(px(260.)).child(actions)),
            )
            .when_some(
                match &state.install {
                    updater::Install::Downloading {
                        received, total, ..
                    } => Some((*received, *total)),
                    _ => None,
                },
                |v, (received, total)| v.child(ui::progress_bar(received, total, theme)),
            )
            .when_some(blocked, |v, why| {
                v.child(
                    div()
                        .text_size(px(11.))
                        .text_color(p.header_muted)
                        .child(why),
                )
            })
            .when_some(notes.filter(|n| !n.is_empty()), |v, notes| {
                v.child(
                    div()
                        .text_size(px(11.))
                        .text_color(p.header_muted)
                        .child(notes),
                )
            })
            .child(self.setting_row(
                text::INCLUDE_PRERELEASES,
                if prerelease_build {
                    text::PRERELEASE_BUILD
                } else {
                    text::PRERELEASES_DESCRIPTION
                },
                self.switch(
                    "updates-prereleases",
                    text::INCLUDE_PRERELEASES,
                    updates.include_prereleases || prerelease_build,
                    !prerelease_build,
                    |p| p.updates.include_prereleases = !p.updates.include_prereleases,
                    theme,
                    cx,
                ),
                theme,
            ))
            .into_any_element()
    }

    fn appearance_section(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        self.setting_row(
            text::APPEARANCE,
            text::APPEARANCE_DESCRIPTION,
            div()
                .map(|v| {
                    #[cfg(all(
                        debug_assertions,
                        feature = "headless-renders",
                        any(target_os = "macos", target_os = "windows")
                    ))]
                    {
                        let bounds = self.appearance_bounds.clone();
                        v.on_children_prepainted(move |children, _, _| {
                            bounds.set(children.try_into().ok())
                        })
                    }
                    #[cfg(not(all(
                        debug_assertions,
                        feature = "headless-renders",
                        any(target_os = "macos", target_os = "windows")
                    )))]
                    {
                        v
                    }
                })
                .id("settings-theme")
                .role(gpui::Role::RadioGroup)
                .aria_label(text::THEME)
                .flex()
                .gap_2()
                .children(
                    [
                        (ThemeChoice::System, text::SYSTEM, "settings-theme-system"),
                        (ThemeChoice::Light, text::LIGHT, "settings-theme-light"),
                        (ThemeChoice::Dark, text::DARK, "settings-theme-dark"),
                    ]
                    .into_iter()
                    .map(|(choice, label, id)| {
                        let selected = self.theme_choice == choice;
                        self.segment(id, label, selected, theme, cx)
                            .flex_1()
                            .flex()
                            .justify_center()
                            .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                            .child(
                                div()
                                    .when(cfg!(test) && selected, |v| {
                                        v.debug_selector(move || format!("{id}-selected"))
                                    })
                                    .child(label),
                            )
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.choose_theme(choice, cx)),
                            )
                    }),
                ),
            theme,
        )
    }

    /// Whether rows continue above and below the page's viewport, from its
    /// last layout. The scroll thumb redraws when that layout changes.
    pub(crate) fn scroll_cues(&self) -> (bool, bool) {
        let max = self.scroll.max_offset().y;
        let offset = -self.scroll.offset().y;
        (offset > px(0.5), max > px(1.) && offset < max - px(0.5))
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
        // reads as a tinted on one.
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
                    this.flip_switch(flip, cx);
                }
            }))
            .into_any_element()
    }

    /// One choice of a segmented control (Appearance, Resolution). The
    /// selected choice is filled with the accent, so it never reads as the
    /// focused one, which only gains the outside ring. The caller
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
                text::CHECK_SPELLING,
                text::CHECK_SPELLING_DESCRIPTION,
                s.enabled,
                true,
                (|p: &mut Preferences| p.spelling.enabled = !p.spelling.enabled)
                    as fn(&mut Preferences),
            ),
            (
                "spelling-english",
                text::ENGLISH,
                text::ENGLISH_DESCRIPTION,
                s.english,
                s.enabled,
                (|p: &mut Preferences| p.spelling.english = !p.spelling.english)
                    as fn(&mut Preferences),
            ),
            (
                "spelling-russian",
                text::RUSSIAN,
                text::RUSSIAN_DESCRIPTION,
                s.russian,
                s.enabled,
                (|p: &mut Preferences| p.spelling.russian = !p.spelling.russian)
                    as fn(&mut Preferences),
            ),
            (
                "spelling-caps",
                text::CHECK_ALL_CAPS,
                text::CHECK_ALL_CAPS_DESCRIPTION,
                s.check_all_caps,
                s.enabled,
                (|p: &mut Preferences| p.spelling.check_all_caps = !p.spelling.check_all_caps)
                    as fn(&mut Preferences),
            ),
            (
                "spelling-digits",
                text::CHECK_DIGITS,
                text::CHECK_DIGITS_DESCRIPTION,
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

    /// Categories to detect: one chip per category scans can
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
            .child(div().text_size(px(13.)).child(text::CATEGORIES_TO_DETECT))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.header_muted)
                    .child(text::CATEGORIES_TO_DETECT_DESCRIPTION),
            )
            .child(
                div()
                    .id("settings-detect-chips")
                    .role(gpui::Role::Group)
                    .aria_label(text::CATEGORIES_TO_DETECT)
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
                text::DETECTION_THRESHOLD,
                text::THRESHOLD_DESCRIPTION,
                &self.threshold,
            )
        } else {
            (
                "settings-confidence-field",
                text::MINIMUM_CONFIDENCE,
                text::CONFIDENCE_DESCRIPTION,
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
            text::APP_NAME,
            text::version(
                env!("CARGO_PKG_VERSION"),
                // A test build is never signed; asking macOS reads the whole
                // folder the test binary sits in (#510).
                if cfg!(test) {
                    text::DEV_BUILD
                } else {
                    mdoc_store::os_key::build_identity()
                },
            ),
            div(),
            theme,
        ));
        for (id, title, description, url) in [
            (
                "about-help",
                text::HELP,
                text::HELP_DESCRIPTION,
                crate::commands::menus::HELP_URL,
            ),
            (
                "about-website",
                text::WEBSITE,
                text::WEBSITE_DESCRIPTION,
                "https://github.com/lawkitt/mdoc",
            ),
            (
                "about-issue",
                text::REPORT_ISSUE,
                text::REPORT_ISSUE_DESCRIPTION,
                "https://github.com/lawkitt/mdoc/issues/new",
            ),
            (
                "about-licence",
                text::APP_LICENSE,
                text::APP_LICENSE_DESCRIPTION,
                "https://github.com/lawkitt/mdoc/blob/main/LICENSE",
            ),
        ] {
            rows = rows.child(
                self.setting_row(
                    title,
                    description,
                    self.scrolled_quiet(id, text::OPEN_LINK, theme, true, cx)
                        .aria_label(title)
                        .on_click(move |_, _, cx| cx.open_url(url)),
                    theme,
                ),
            );
        }
        rows.child(
            div()
                .py_3()
                .text_size(px(11.))
                .text_color(theme.pdf_style().header_muted)
                .child(text::PRIVACY_NOTE),
        )
        .into_any_element()
    }

    fn category_body(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let body = match self.category() {
            Category::General => div()
                .child(self.appearance_section(theme, cx))
                // MacOS; Windows only while the preview is on (its tray).
                .when(
                    crate::shelf::tray::setting_shown(self.draft.ai_connector.preview),
                    |v| {
                        v.child(self.setting_row(
                            crate::shelf::tray::SETTING,
                            text::KEEP_RUNNING,
                            self.switch(
                                "general-menu-bar",
                                crate::shelf::tray::SETTING,
                                self.draft.general.menu_bar,
                                true,
                                |p| p.general.menu_bar = !p.general.menu_bar,
                                theme,
                                cx,
                            ),
                            theme,
                        ))
                    },
                )
                .child(self.updates_section(theme, cx))
                .child(
                    self.setting_row(
                        text::KEYBOARD_SHORTCUTS,
                        text::KEYBOARD_SHORTCUTS_DESCRIPTION,
                        self.scrolled_quiet("settings-shortcuts", text::SHOW, theme, true, cx)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "settings-shortcuts".into())
                            })
                            .aria_label(text::KEYBOARD_SHORTCUTS)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Event::Shortcuts))),
                        theme,
                    ),
                ),
            Category::Detection => div()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme.pdf_style().header_muted)
                        .child(self.section_note(
                            text::DETECTION_NOTE,
                            Model::Pii(self.draft.pseudonymization.model),
                        )),
                )
                .child(
                    div()
                        .id("pii-models")
                        .role(gpui::Role::RadioGroup)
                        .aria_label(text::PSEUDONYMIZATION)
                        .children(
                            PiiModel::ALL
                                .into_iter()
                                .map(|m| self.row(Model::Pii(m), cx)),
                        ),
                )
                .child(self.number_row(0, theme, cx))
                .child(self.categories_row(theme, cx))
                .child(self.user_lists_section(theme, cx))
                .child(
                    self.setting_row(
                        text::COMPARE_PSEUDONYMIZATION,
                        text::COMPARE_DESCRIPTION,
                        self.scrolled_quiet("compare-pii", text::COMPARE, theme, true, cx)
                            .aria_label(text::COMPARE_PSEUDONYMIZATION)
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(Event::Compare(Model::Pii(PiiModel::Fp16)))
                            })),
                        theme,
                    ),
                ),
            Category::Recognition => {
                let note =
                    self.section_note(text::RECOGNITION_NOTE, Model::Ocr(self.draft.ocr.model));
                div()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme.pdf_style().header_muted)
                            .child(note.clone())
                            .when(cfg!(test), |v| v.debug_selector(move || note.clone())),
                    )
                    .child(
                        div()
                            .id("ocr-models")
                            .role(gpui::Role::RadioGroup)
                            .aria_label(text::TEXT_RECOGNITION)
                            .children(
                                OcrModel::ALL
                                    .into_iter()
                                    .map(|m| self.row(Model::Ocr(m), cx)),
                            ),
                    )
                    .child(
                        self.setting_row(
                            text::RESOLUTION,
                            text::RESOLUTION_DESCRIPTION,
                            div()
                                .id("settings-dpi")
                                .role(gpui::Role::RadioGroup)
                                .aria_label(text::RESOLUTION)
                                .flex()
                                .gap_2()
                                .children([150, 200, 300].map(|dpi| {
                                    self.segment(
                                        ("dpi", dpi as usize),
                                        text::dpi(dpi),
                                        self.draft.ocr.dpi == dpi,
                                        theme,
                                        cx,
                                    )
                                    .child(format!("{dpi}"))
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(move || format!("settings-dpi-{dpi}"))
                                    })
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.set_dpi(dpi, cx)),
                                    )
                                })),
                            theme,
                        ),
                    )
                    .child(self.number_row(1, theme, cx))
                    .child(
                        self.setting_row(
                            text::COMPARE_TEXT_RECOGNITION,
                            text::COMPARE_DESCRIPTION,
                            self.scrolled_quiet("compare-ocr", text::COMPARE, theme, true, cx)
                                .aria_label(text::COMPARE_TEXT_RECOGNITION)
                                .on_click(cx.listener(|_, _, _, cx| {
                                    cx.emit(Event::Compare(Model::Ocr(OcrModel::Cyrillic)))
                                })),
                            theme,
                        ),
                    )
            }
            Category::Spelling => div().child(self.spelling_section(theme, cx)),
            Category::Vault => div().child(self.vault_section(theme, cx)),
            Category::AiConnector => div().child(self.ai_connector_section(theme, cx)),
            Category::About => div().child(self.about_section(theme, cx)),
        };
        // About and Vault have no preferences, so nothing to reset; the AI
        // connector's one switch is its own reset.
        body.when(
            !matches!(
                self.category,
                Category::About | Category::Vault | Category::AiConnector
            ),
            |v| {
                v.child(
                    self.setting_row(
                        text::RESET,
                        text::RESET_DESCRIPTION,
                        self.scrolled_quiet("reset-settings", text::RESET, theme, true, cx)
                            .when(cfg!(test), |v| v.debug_selector(|| "reset-settings".into()))
                            .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
                        theme,
                    ),
                )
            },
        )
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
            .aria_label(text::SETTINGS_CATEGORIES)
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
                            quiet_control("close-dictionary", text::BACK_TO_SPELLING, theme, true)
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
                                text::YOUR_DICTIONARY
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
                // fades at an edge where rows continue.
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
                    .aria_label(text::SETTINGS)
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
                                    .child(text::SETTINGS),
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
                            .aria_label(text::CLOSE_SETTINGS)
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
