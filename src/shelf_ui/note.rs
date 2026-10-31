//! The first-close note (ADR 0084): the first time the last window closes
//! with the menu bar item on, a note under the icon says mdoc is still
//! there. **Got it** closes it; **Quit when windows close** turns the
//! switch off and quits as mdoc now would. It never shows again.

use super::CloseShelf;
use crate::{menu_bar, style::Theme, ui, workspace::Workspace};
use gpui::{
    App, Context, FocusHandle, Focusable, Render, Window, WindowHandle, div, prelude::*, px,
};
use std::{cell::Cell, rc::Rc};

pub(crate) const NOTE_WIDTH: f32 = 300.;
pub(crate) const NOTE_HEIGHT: f32 = 108.;
pub(crate) const NOTE_TITLE: &str = "mdoc is still in the menu bar.";
pub(crate) const NOTE_TEXT: &str = "Your documents are here.";

pub(crate) struct NoteView {
    workspace: WindowHandle<Workspace>,
    theme: Rc<Cell<Theme>>,
    focus: FocusHandle,
    got_it: FocusHandle,
}

impl NoteView {
    pub(crate) fn new(
        workspace: WindowHandle<Workspace>,
        theme: Rc<Cell<Theme>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle().tab_stop(false);
        let got_it = cx.focus_handle().tab_stop(true);
        window.focus(&got_it, cx);
        Self {
            workspace,
            theme,
            focus,
            got_it,
        }
    }

    pub(crate) fn got_it(&mut self, window: &mut Window, cx: &mut App) {
        window.remove_window();
        menu_bar::note_closed(cx);
    }

    /// Turn the switch off and quit, as mdoc does with its last window
    /// closed when the item is off.
    pub(crate) fn quit_when_windows_close(&mut self, window: &mut Window, cx: &mut App) {
        self.got_it(window, cx);
        let workspace = self.workspace;
        cx.defer(move |cx| {
            let _ = workspace.update(cx, |workspace, window, cx| {
                workspace.quit_when_windows_close(window, cx)
            });
        });
    }
}

impl Focusable for NoteView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NoteView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        div()
            .id("shelf-note")
            .key_context("Shelf")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &CloseShelf, window, cx| this.got_it(window, cx)))
            .on_action(
                cx.listener(|_, _: &ui::NextControl, window, cx| {
                    ui::cycle(window, cx, None, false)
                }),
            )
            .on_action(cx.listener(|_, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, None, true)
            }))
            .size_full()
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .bg(super::surface(theme))
            .text_color(p.header_fg)
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(NOTE_TITLE),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(NOTE_TEXT),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        ui::control("shelf-note-quit", "Quit when windows close", theme, true)
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .text_size(px(12.))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.quit_when_windows_close(window, cx)
                            })),
                    )
                    .child(
                        ui::primary_button("shelf-note-got-it", "Got it", theme, true)
                            .track_focus(&self.got_it)
                            .on_click(cx.listener(|this, _, window, cx| this.got_it(window, cx))),
                    ),
            )
    }
}
