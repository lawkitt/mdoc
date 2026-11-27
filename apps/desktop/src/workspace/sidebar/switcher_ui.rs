//! The recency switcher in the window: `⌃Tab` opens it over the live views,
//! more `⌃Tab`s move on, and releasing `⌃` (or Enter, or a click) shows the
//! pick; Esc or a click outside leaves things as they were.
use crate::ui;
use crate::workspace::{Workspace, sidebar::switcher::Switcher};
use gpui::{AnyElement, App, Context, KeyBinding, Window, actions, div, prelude::*, px};

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
    /// `⌃Tab` / `⌃⇧Tab`: open the switcher, or move its pick.
    pub(in crate::workspace) fn switch_document(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.quitting() {
            return;
        }
        if let Some(switcher) = &mut self.switcher {
            switcher.step(if backwards { -1 } else { 1 });
            cx.notify();
            return;
        }
        if self.dialog_up(window)
            || self.settings.read(cx).open
            || self.shell.palette.is_some()
            || self.project_ui.list_open
        {
            return;
        }
        let views = self.tabs.recent(|tab| {
            if self.is_placeholder(tab, cx) {
                crate::workspace::saving::live_views::Weight::Ignored
            } else {
                crate::workspace::saving::live_views::Weight::Idle
            }
        });
        let shown = !self.showing_overview(cx);
        let Some(switcher) = Switcher::open(views, shown, backwards) else {
            return;
        };
        self.switcher = Some(switcher);
        self.switcher_return = window.focused(cx);
        window.focus(&self.switcher_focus, cx);
        cx.notify();
    }

    /// Show the switcher's pick.
    pub(in crate::workspace) fn confirm_switcher(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(switcher) = self.switcher.take() else {
            return;
        };
        self.switcher_return = None;
        let picked = switcher.picked();
        if self.tabs.contains(picked) {
            self.activate(picked, window, cx);
        }
        cx.notify();
    }

    /// The live views as the Window menu lists them: by title, in the
    /// order they opened (#1160). Which one is shown is left out, so
    /// switching documents leaves the menu bar as it is.
    pub(in crate::workspace) fn live_view_items(
        &self,
        cx: &App,
    ) -> Vec<crate::commands::menus::LiveViewItem> {
        let mut views: Vec<_> = (self.tabs.iter())
            .filter(|tab| !self.is_placeholder(tab, cx))
            .map(|tab| crate::commands::menus::LiveViewItem {
                id: tab.id(),
                title: crate::workspace::tab_label(tab, cx).name.into(),
            })
            .collect();
        views.sort_by_key(|view| view.id);
        views
    }

    /// Window › a live view's title: the window comes forward and shows
    /// it, as Back and the switcher would. Nothing moves under a dialog,
    /// sheet, Settings, the palette or the Project list, or while
    /// quitting; one no longer live does nothing.
    pub(in crate::workspace) fn show_live_view(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.quit_flow.quitting()
            || self.modal_up(window, cx)
            || self.shell.palette.is_some()
            || self.project_ui.list_open
            || self.switcher.is_some()
            || !self.tabs.contains(id)
        {
            return;
        }
        crate::shelf::menu_bar::reveal_window(window, cx);
        self.activate(id, window, cx);
        cx.notify();
    }

    /// Esc or a click outside: nothing changes.
    pub(in crate::workspace) fn cancel_switcher(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.switcher.take().is_none() {
            return;
        }
        if let Some(focus) = self.switcher_return.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

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
                            .aria_label("Recent documents")
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
                                    .child("Recent documents"),
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
