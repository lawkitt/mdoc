//! The document header's Ask (#1164 Decision 22; Milestone 2 item 10),
//! only while "AI connector (preview)" is on and the document is in the
//! Vault: a split button whose main part repeats the last choice, a menu
//! of Ask Claude, Ask ChatGPT and Copy for AI, and, for an Unfiled
//! document, the card offering "Move to a new Project and share". The
//! window carries each choice out (`TabEvent::Ask`).
use super::DocumentView;
use crate::{ai_ask, ui, workspace::TabEvent};
use gpui::{App, Context, Window, div, prelude::*, px};
use mdoc_store::settings::AskWith;
use std::{cell::Cell, rc::Rc};

/// The menu and the Unfiled card, while either shows.
pub(crate) struct AskHeader {
    pub(crate) open: bool,
    /// Ask on an Unfiled document: the card for this app shows.
    pub(crate) unfiled: Option<AskWith>,
    /// The open menu or card holds focus, so Tab and Esc work in it.
    focus: gpui::FocusHandle,
    /// The button's chevron, which focus returns to.
    more_focus: gpui::FocusHandle,
    /// The chevron's bounds, so a press on it is left to its toggle.
    bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
}

impl AskHeader {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            open: false,
            unfiled: None,
            focus: cx.focus_handle(),
            more_focus: cx.focus_handle().tab_stop(true),
            bounds: Default::default(),
        }
    }
}

impl DocumentView {
    /// The last choice while Ask shows: the preview on, a Vault document
    /// with Markdown.
    pub(crate) fn ask_shown(&self) -> Option<AskWith> {
        let ai = self.preferences.borrow().current.as_ref()?.ai_connector;
        let shown = ai.preview
            && self.session.document.vault.is_some()
            && self.can_copy_markdown()
            && !self.lifecycle.source_only();
        shown.then_some(ai.ask)
    }

    /// Choice `with`, from the button or the menu: Unfiled asks first;
    /// otherwise the window carries it out.
    pub(crate) fn ask(&mut self, with: AskWith, window: &mut Window, cx: &mut Context<Self>) {
        self.ask.open = false;
        if with != AskWith::CopyForAi && self.project.link().is_none() {
            self.ask.unfiled = Some(with);
            window.focus(&self.ask.focus, cx);
        } else {
            self.ask.unfiled = None;
            cx.emit(TabEvent::Ask {
                with,
                move_to_new: false,
            });
        }
        cx.notify();
    }

    /// The Unfiled card's "Move to a new Project and share".
    pub(crate) fn move_and_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(with) = self.ask.unfiled.take() else {
            return;
        };
        window.focus(&self.ask.more_focus, cx);
        cx.emit(TabEvent::Ask {
            with,
            move_to_new: true,
        });
        cx.notify();
    }

    /// Close the menu or the card, focus back on the chevron.
    pub(crate) fn close_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ask.open = false;
        self.ask.unfiled = None;
        window.focus(&self.ask.more_focus, cx);
        cx.notify();
    }

    /// Renders: the menu open, or the Unfiled card for `unfiled`'s app.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(crate) fn show_ask(
        &mut self,
        unfiled: Option<AskWith>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ask.open = unfiled.is_none();
        self.ask.unfiled = unfiled;
        window.focus(&self.ask.focus, cx);
        cx.notify();
    }

    /// Copy for AI is off where the ⋯ menu's is.
    fn ask_enabled(&self, with: AskWith) -> bool {
        with != AskWith::CopyForAi || self.alias_actions_reason().is_none()
    }

    /// The split button, with its menu or card when open.
    pub(super) fn ask_button(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let last = self.ask_shown()?;
        let theme = self.theme.get();
        let enabled = self.ask_enabled(last);
        let main = ui::control("header-ask", ai_ask::label(last), theme, enabled)
            .when(cfg!(test), |v| v.debug_selector(|| "header-ask".into()))
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.ask_enabled(last) {
                    this.ask(last, window, cx)
                }
            }));
        let open = self.ask.open || self.ask.unfiled.is_some();
        let (frame, more) =
            ui::split_button(main, "header-ask-more", "More ways to ask", open, theme);
        let bounds = self.ask.bounds.clone();
        let popup = if self.ask.unfiled.is_some() {
            Some(self.ask_unfiled_card(cx))
        } else if self.ask.open {
            Some(self.ask_menu(last, cx))
        } else {
            None
        };
        Some(
            div()
                .relative()
                .child(
                    frame.child(
                        div()
                            .relative()
                            .child(
                                gpui::canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                                    .absolute()
                                    .inset_0(),
                            )
                            .child(
                                more.track_focus(&self.ask.more_focus)
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "header-ask-more".into())
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let open = !(this.ask.open || this.ask.unfiled.is_some());
                                        this.ask.open = open;
                                        this.ask.unfiled = None;
                                        this.more_open = false;
                                        if open {
                                            window.focus(&this.ask.focus, cx);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
                )
                // Hung from the button's right edge, so it opens under it.
                .children(popup.map(|popup| div().absolute().top_0().right_0().child(popup)))
                .into_any_element(),
        )
    }

    /// A panel under the button that holds focus and closes on Esc or a
    /// press outside it.
    fn ask_panel(&self, id: &'static str, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme.get();
        ui::panel(id, theme)
            .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
            .track_focus(&self.ask.focus)
            .key_context("UiPanel UiMenu")
            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                ui::cycle(window, cx, Some(&this.ask.focus), false);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, Some(&this.ask.focus), true);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                this.close_ask(window, cx);
                cx.stop_propagation();
            }))
            .mt(px(32.))
            .p_1()
            .rounded_lg()
            .shadow_lg()
            .on_mouse_down_out(
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    // A press on the chevron is its toggle's.
                    if this.ask.bounds.get().contains(&event.position) {
                        return;
                    }
                    this.ask.open = false;
                    this.ask.unfiled = None;
                    let panel = this.ask.focus.clone();
                    let button = this.ask.more_focus.clone();
                    window.defer(cx, move |window, cx| {
                        if panel.contains_focused(window, cx) {
                            window.focus(&button, cx);
                        }
                    });
                    cx.notify();
                }),
            )
    }

    fn anchored(panel: gpui::Stateful<gpui::Div>) -> gpui::AnyElement {
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::TopRight)
                .snap_to_window_with_margin(px(8.))
                .child(panel),
        )
        .with_priority(1)
        .into_any_element()
    }

    /// Ask Claude, Ask ChatGPT and Copy for AI, the last choice ticked.
    fn ask_menu(&self, last: AskWith, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let reason = self.alias_actions_reason();
        let mut menu = self.ask_panel("ask-menu", cx);
        for with in ai_ask::CHOICES {
            let id = match with {
                AskWith::Claude => "ask-claude",
                AskWith::ChatGpt => "ask-chatgpt",
                AskWith::CopyForAi => "ask-copy-for-ai",
            };
            let enabled = self.ask_enabled(with);
            if with == AskWith::CopyForAi {
                menu = menu.child(ui::menu_divider(theme));
            }
            menu = menu.child(
                ui::menu_row_because(
                    id,
                    ai_ask::label(with),
                    None,
                    Some(with == last),
                    theme,
                    enabled,
                    (with == AskWith::CopyForAi)
                        .then_some(reason)
                        .flatten()
                        .map(Into::into),
                )
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if enabled {
                        this.ask(with, window, cx)
                    }
                })),
            );
        }
        Self::anchored(menu)
    }

    /// Ask on an Unfiled document: move it to a new Project and share
    /// that, or not.
    fn ask_unfiled_card(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let with = self.ask.unfiled.unwrap_or_default();
        let title = ai_ask::unfiled_title(with);
        let card = self
            .ask_panel("ask-unfiled", cx)
            .role(gpui::Role::Group)
            .aria_label(title.clone())
            .w(px(360.))
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(ui::card_text(ai_ask::UNFILED_TEXT, theme))
            .child(
                ui::card_actions()
                    .justify_end()
                    .child(
                        ui::action_control("ask-unfiled-cancel", "Cancel", theme, true, false)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "ask-unfiled-cancel".into())
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_ask(window, cx)),
                            ),
                    )
                    .child(
                        ui::primary_button(
                            "ask-move-and-share",
                            ai_ask::MOVE_AND_SHARE,
                            theme,
                            true,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "ask-move-and-share".into())
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.move_and_ask(window, cx))),
                    ),
            );
        Self::anchored(card)
    }
}
