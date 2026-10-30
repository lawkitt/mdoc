//! The shelf's view (ADR 0084): the borderless popup under the menu bar
//! item. It closes on a click outside (the panel loses key status) or Esc,
//! and after an action that brings mdoc forward.

use crate::{
    menu_bar,
    style::Theme,
    ui,
    workspace::{Quit, Workspace},
};
use gpui::{
    App, Context, FocusHandle, Focusable, KeyBinding, Render, Window, WindowHandle, actions, div,
    prelude::*, px,
};
use std::{cell::Cell, rc::Rc};

actions!(shelf, [CloseShelf]);

/// Keys inside the shelf (plumbing, ADR 0073: they move or close within it).
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CloseShelf, Some("Shelf"))]);
}

const ROW_HEIGHT: f32 = 32.;
const PADDING: f32 = 6.;

pub(crate) struct ShelfView {
    /// The shelf lists this workspace's documents (the next layer).
    _workspace: WindowHandle<Workspace>,
    theme: Rc<Cell<Theme>>,
    focus: FocusHandle,
}

impl ShelfView {
    /// The shelf's height for what it lists now.
    pub(crate) fn height(_workspace: WindowHandle<Workspace>, _cx: &App) -> f32 {
        2. * ROW_HEIGHT + 2. * PADDING
    }

    pub(crate) fn new(
        workspace: WindowHandle<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = workspace
            .read(cx)
            .map(|workspace| workspace.theme_cell())
            .unwrap_or_default();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // A click outside takes key status from the panel: close it.
        cx.observe_window_activation(window, |_, window, cx| {
            if !window.is_window_active() {
                window.remove_window();
                menu_bar::shelf_closed(cx);
            }
        })
        .detach();
        Self {
            _workspace: workspace,
            theme,
            focus,
        }
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        menu_bar::shelf_closed(cx);
    }

    fn show_mdoc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
        cx.defer(|cx| menu_bar::reveal(cx, |_, _, _| {}));
    }

    fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
        // The quit flow asks about unsaved work as ⌘Q does (ADR 0071).
        cx.defer(|cx| cx.dispatch_action(&Quit));
    }
}

impl Focusable for ShelfView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ShelfView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let row = |id: &'static str, label: &'static str| {
            ui::control(id, label, theme, true)
                .h(px(ROW_HEIGHT))
                .w_full()
                .flex()
                .items_center()
                .aria_label(label)
        };
        div()
            .id("shelf")
            .key_context("Shelf")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &CloseShelf, window, cx| this.close(window, cx)))
            .size_full()
            .flex()
            .flex_col()
            .p(px(PADDING))
            .bg(p.bg)
            .text_color(p.header_fg)
            .text_size(px(13.))
            .child(
                row("shelf-show", "Show mdoc")
                    .on_click(cx.listener(|this, _, window, cx| this.show_mdoc(window, cx))),
            )
            .child(
                row("shelf-quit", "Quit mdoc")
                    .on_click(cx.listener(|this, _, window, cx| this.quit(window, cx))),
            )
    }
}
