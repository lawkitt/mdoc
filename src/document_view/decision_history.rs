//! The header's clock button and its What you did popover (ADR 0079): one
//! line per pseudonymization decision; a click travels the undo history to
//! it. Keyboard reach and Esc follow the More menu (ADR 0062).
use super::DocumentView;
use crate::ui;
use gpui::{App, Context, FocusHandle, Window, div, prelude::*, px};
use mdoc_pii::decisions::EntryState;
use std::{cell::Cell, rc::Rc};

/// The popover's view state: whether it is open, its button and its focus.
pub(crate) struct HistoryPopover {
    pub(crate) open: bool,
    /// The clock button, which closing gives focus back to.
    pub(crate) button: FocusHandle,
    /// The open popover, which holds focus and Tab.
    pub(crate) menu: FocusHandle,
    /// The button's bounds, so a press on it is left to its toggle.
    bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
}

impl HistoryPopover {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self {
            open: false,
            button: cx.focus_handle().tab_stop(true),
            menu: cx.focus_handle(),
            bounds: Default::default(),
        }
    }
}

/// The footer note: travel takes text edits with it (ADR 0079).
pub(crate) const FOOTER: &str = "Text edits made between decisions are undone with them.";

impl DocumentView {
    /// The palette's What you did, and the clock button: open the popover
    /// with focus in it.
    pub(crate) fn open_decision_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.more_open = false;
        self.history.open = true;
        window.focus(&self.history.menu, cx);
        cx.notify();
    }
    /// Close the popover, focus back on the clock button.
    pub(crate) fn close_decision_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.history.open = false;
        window.focus(&self.history.button, cx);
        cx.notify();
    }
    /// The clock button shows once there is something to decide or decided.
    pub(super) fn shows_history_button(&self, reviewable: bool, cx: &App) -> bool {
        let path = self.editor.read(cx).history_path();
        reviewable || self.history.open || self.pii.decisions().rows(&path).next().is_some()
    }

    pub(super) fn history_button(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let open = self.history.open;
        let popover = open.then(|| self.history_popover(cx));
        let bounds = self.history.bounds.clone();
        div()
            .relative()
            .child(
                gpui::canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .child(
                ui::header_icon(
                    "header-history",
                    "What you did",
                    ui::Icon::Clock,
                    theme,
                    open,
                )
                .track_focus(&self.history.button)
                .when(cfg!(test), |v| v.debug_selector(|| "header-history".into()))
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.history.open {
                        this.history.open = false;
                        cx.notify();
                    } else {
                        this.open_decision_history(window, cx);
                    }
                })),
            )
            .children(popover)
            .into_any_element()
    }

    fn history_popover(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let path = self.editor.read(cx).history_path();
        let rows: Vec<_> = self
            .pii
            .decisions()
            .rows(&path)
            .map(|(entry, state)| (entry.decision().label(), entry.time().to_owned(), state))
            .collect();
        let empty = rows.is_empty();
        let list = rows
            .into_iter()
            .enumerate()
            .map(|(i, (label, time, state))| {
                let undone = state == EntryState::Undone;
                let id = format!("decision-{i}");
                ui::focus_ring(gpui::SharedString::from(id.clone()), theme)
                    .when(cfg!(test), move |v| v.debug_selector(move || id))
                    .role(gpui::Role::MenuItem)
                    .aria_label(if undone {
                        format!("{label} · {time}, undone")
                    } else {
                        format!("{label} · {time}")
                    })
                    .aria_description(if undone {
                        "Redo up to here"
                    } else {
                        "Undo back to just after this"
                    })
                    .key_context("UiControl")
                    .tab_index(0)
                    .flex()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .px_2()
                    .min_h(px(28.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.placeholder_bg))
                    .text_color(if undone { p.header_muted } else { p.header_fg })
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .when(undone, |v| {
                        v.child(div().flex_shrink_0().text_size(px(11.)).child("Undone"))
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_color(p.header_muted)
                            .child(format!("· {time}")),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.travel_to_decision(i, window, cx);
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::TopRight)
                .snap_to_window_with_margin(px(8.))
                .child(
                    ui::panel("decision-history", theme)
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "decision-history".into())
                        })
                        .track_focus(&self.history.menu)
                        .key_context("UiPanel UiMenu")
                        .role(gpui::Role::Menu)
                        .aria_label("What you did")
                        .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.history.menu), false);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                            ui::cycle(window, cx, Some(&this.history.menu), true);
                            cx.stop_propagation();
                        }))
                        .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                            this.close_decision_history(window, cx);
                            cx.stop_propagation();
                        }))
                        .on_mouse_down_out(cx.listener(
                            |this, event: &gpui::MouseDownEvent, window, cx| {
                                // A press on the clock button is its toggle's.
                                if this.history.bounds.get().contains(&event.position) {
                                    return;
                                }
                                this.history.open = false;
                                let menu = this.history.menu.clone();
                                let button = this.history.button.clone();
                                window.defer(cx, move |window, cx| {
                                    if menu.contains_focused(window, cx) {
                                        window.focus(&button, cx);
                                    }
                                });
                                cx.notify();
                            },
                        ))
                        .mt(px(32.))
                        .w(px(340.))
                        .p_1()
                        .rounded_lg()
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .px(px(12.))
                                .pt_1()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child("What you did"),
                        )
                        .when(empty, |v| {
                            v.child(
                                div()
                                    .px(px(12.))
                                    .pt_1()
                                    .text_color(p.header_muted)
                                    .child("Your pseudonymization decisions appear here."),
                            )
                        })
                        .child(
                            div()
                                .id("decision-history-list")
                                // Room for the rows' outside focus ring.
                                .p(px(gpui_pdf::FOCUS_RING_OUTSET))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .max_h(px(360.))
                                .overflow_y_scroll()
                                .children(list),
                        )
                        .child(ui::menu_divider(theme))
                        .child(
                            div()
                                .px(px(12.))
                                .pb_1()
                                .text_size(px(12.))
                                .text_color(p.header_muted)
                                .child(FOOTER),
                        ),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }
}
