//! The recency switcher's view over the live views, and its keys; what
//! `⌃Tab`, releasing `⌃`, Enter or Esc carries out is in `actions.rs`.
use crate::ui;
use crate::workspace::Workspace;
use gpui::{AnyElement, App, Context, KeyBinding, actions, div, prelude::*, px};

actions!(mdoc_switcher, [SwitcherUp, SwitcherDown, SwitcherConfirm]);

const SWITCHER: &str = "RecencySwitcher";
const SWITCHER_CONTEXT: &str = "UiPanel UiMenu RecencySwitcher";

/// Moving and confirming inside the open switcher; Tab, Shift+Tab and Esc
/// are the menu's own (`UiPanel UiMenu`).
pub(in crate::workspace) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SwitcherUp, Some(SWITCHER)),
        KeyBinding::new("down", SwitcherDown, Some(SWITCHER)),
        KeyBinding::new("enter", SwitcherConfirm, Some(SWITCHER)),
    ]);
}

impl Workspace {
    pub(in crate::workspace) fn switcher_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let switcher = self.switcher.as_ref()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let picked = switcher.picked();
        let rows: Vec<AnyElement> = (switcher.views().iter())
            .filter_map(|id| self.tabs.get(*id))
            .map(|tab| {
                let id = tab.id();
                let label = crate::workspace::tab_label(tab, cx);
                let chosen = id == picked;
                div()
                    .id(("switcher-row", id))
                    .when(cfg!(test), |v| {
                        v.debug_selector(move || format!("switcher-{id}"))
                    })
                    .role(gpui::Role::ListBoxOption)
                    .aria_selected(chosen)
                    .aria_label(label.name.clone())
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .when(chosen, |v| v.bg(theme.sidebar_selected()))
                    .hover(move |v| v.bg(palette.placeholder_bg))
                    .cursor_pointer()
                    .child(
                        div()
                            .min_w(px(36.))
                            .text_size(px(10.))
                            .text_color(palette.header_muted)
                            .child(label.format),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(label.name))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(switcher) = &mut this.switcher {
                            switcher.pick(id);
                        }
                        this.confirm_switcher(window, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        ui::panel("recency-switcher", theme)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "recency-switcher".into())
                            })
                            .key_context(SWITCHER_CONTEXT)
                            .track_focus(&self.switcher_focus)
                            .role(gpui::Role::ListBox)
                            .aria_label(super::text::RECENT_DOCUMENTS)
                            .w(px(360.))
                            .p_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .px_3()
                                    .pt_1()
                                    .pb_1()
                                    .text_size(px(11.))
                                    .text_color(palette.header_muted)
                                    .child(super::text::RECENT_DOCUMENTS),
                            )
                            .children(rows)
                            .on_modifiers_changed(cx.listener(
                                |this, event: &gpui::ModifiersChangedEvent, window, cx| {
                                    if !event.modifiers.control {
                                        this.confirm_switcher(window, cx);
                                    }
                                },
                            ))
                            .on_action(cx.listener(|this, _: &SwitcherUp, _, cx| {
                                if let Some(switcher) = &mut this.switcher {
                                    switcher.step(-1);
                                }
                                cx.notify();
                            }))
                            .on_action(cx.listener(|this, _: &SwitcherDown, _, cx| {
                                if let Some(switcher) = &mut this.switcher {
                                    switcher.step(1);
                                }
                                cx.notify();
                            }))
                            .on_action(cx.listener(|this, _: &ui::NextControl, _, cx| {
                                if let Some(switcher) = &mut this.switcher {
                                    switcher.step(1);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }))
                            .on_action(cx.listener(|this, _: &ui::PreviousControl, _, cx| {
                                if let Some(switcher) = &mut this.switcher {
                                    switcher.step(-1);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }))
                            .on_action(cx.listener(|this, _: &SwitcherConfirm, window, cx| {
                                this.confirm_switcher(window, cx)
                            }))
                            .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                                this.cancel_switcher(window, cx)
                            }))
                            .on_mouse_down_out(cx.listener(
                                |this, _: &gpui::MouseDownEvent, window, cx| {
                                    this.cancel_switcher(window, cx)
                                },
                            ))
                            .text_color(palette.header_fg)
                            .shadow_lg(),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
