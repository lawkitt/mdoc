//! How the shelf's popup draws: the rows or the source picker, Restart to
//! update, the Clipboard section with its source line, and the footer, in
//! one column the popup measures, with the keys it answers.
use super::{
    ACTION_ROW, CLIPBOARD, CLIPBOARD_BUTTONS, CLIPBOARD_HEADING, ClipboardRow, CloseShelf, DIVIDER,
    FOOTER, PADDING, PICKER_HEADING, SECTION_GAP, SOURCE_LINE, ShelfCopyForAi, ShelfCopyRow,
    ShelfDown, ShelfUp, ShelfView, surface,
};
use crate::{shelf::text, ui};
use gpui::{
    AnyElement, Context, ExternalPaths, IntoElement, Render, SharedString, Window, div, prelude::*,
    px,
};

impl ShelfView {
    fn action_row(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> ui::Control {
        let theme = self.theme.get();
        ui::control(id, label, theme, true)
            .h(px(ACTION_ROW))
            .flex()
            .items_center()
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }

    fn divider(&self) -> AnyElement {
        let p = self.theme.get().pdf_style();
        div()
            .h(px(DIVIDER))
            .flex()
            .items_center()
            .px(px(4.))
            .child(div().w_full().h(px(1.)).bg(p.border))
            .into_any_element()
    }

    /// One of the two equal clipboard buttons.
    fn clipboard_button(
        &self,
        id: &'static str,
        label: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> ui::Control {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        ui::control(id, label, theme, enabled)
            .flex_1()
            .h(px(CLIPBOARD_BUTTONS))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(p.border)
            .text_size(px(12.))
            .font_weight(gpui::FontWeight::MEDIUM)
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }

    /// "Clipboard", Pseudonymize and Restore (or one Unlock button for a
    /// locked Vault), then the quiet source line with its ▾.
    fn clipboard_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let enabled = self.source.is_some();
        let buttons = if enabled && self.clipboard_locked(cx) {
            div().flex().flex_row().child(self.clipboard_button(
                "shelf-unlock",
                text::UNLOCK_IN_MDOC,
                true,
                cx,
                |this, window, cx| this.clipboard(ClipboardRow::Restore, window, cx),
            ))
        } else {
            div()
                .flex()
                .flex_row()
                .gap(px(6.))
                .child(self.clipboard_button(
                    "shelf-pseudonymize",
                    text::PSEUDONYMIZE,
                    enabled,
                    cx,
                    |this, window, cx| this.clipboard(ClipboardRow::Pseudonymize, window, cx),
                ))
                .child(self.clipboard_button(
                    "shelf-restore",
                    text::RESTORE,
                    enabled,
                    cx,
                    |this, window, cx| this.clipboard(ClipboardRow::Restore, window, cx),
                ))
        };
        let line = self
            .source_line()
            .unwrap_or_else(|| text::NAMES_APPEAR.into());
        let source = div()
            .h(px(SOURCE_LINE))
            .px(px(4.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .text_size(px(11.))
            .text_color(p.header_muted)
            .child(
                div()
                    .id("shelf-source-line")
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .when(self.result.is_some(), |v| v.text_color(p.header_fg))
                    .child(line),
            )
            .when(self.sources.len() > 1, |v| {
                v.child(
                    ui::control("shelf-pick", "▾", theme, true)
                        .track_focus(&self.pick_button)
                        .aria_label(text::CHOOSE_WHOSE_NAMES)
                        .px(px(4.))
                        .py_0()
                        .text_size(px(11.))
                        .text_color(p.header_muted)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_picker(window, cx)),
                        ),
                )
            });
        div()
            .h(px(CLIPBOARD))
            .px(px(4.))
            .flex()
            .flex_col()
            .gap(px(SECTION_GAP))
            .child(
                div()
                    .h(px(CLIPBOARD_HEADING))
                    .flex()
                    .items_end()
                    .px(px(4.))
                    .text_size(px(11.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(p.header_muted)
                    .child(text::CLIPBOARD),
            )
            .child(buttons)
            .child(source)
            .into_any_element()
    }

    fn picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(PICKER_HEADING))
                    .px_2()
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(p.header_muted)
                    .child(text::CLIPBOARD_NAMES_FROM),
            )
            .children(self.sources.iter().enumerate().map(|(i, source)| {
                let chosen = self.source.as_ref() == Some(source);
                let picked = source.clone();
                ui::control(("shelf-source", i), source.name().to_string(), theme, true)
                    .track_focus(&self.picker_rows[i])
                    .w_full()
                    .h(px(ACTION_ROW))
                    .flex()
                    .items_center()
                    .role(gpui::Role::RadioButton)
                    .aria_toggled(if chosen {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .when(chosen, |v| v.bg(theme.sidebar_selected()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose(picked.clone(), window, cx)
                    }))
            }))
            .into_any_element()
    }

    /// Open mdoc at the left; Settings and Quit at the right.
    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        div()
            .h(px(FOOTER))
            .flex()
            .flex_row()
            .items_center()
            .child(self.action_row(
                "shelf-open-mdoc",
                text::OPEN_MDOC,
                cx,
                |this, window, cx| this.open_mdoc(window, cx),
            ))
            .child(div().flex_1())
            .child(
                ui::header_icon(
                    "shelf-settings",
                    text::SETTINGS,
                    ui::Icon::Settings,
                    theme,
                    false,
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
            )
            .child(
                self.action_row("shelf-quit", text::QUIT, cx, |this, window, cx| {
                    this.quit(window, cx)
                }),
            )
            .into_any_element()
    }
}

impl Render for ShelfView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let list: AnyElement = if self.picking {
            self.picker(cx)
        } else {
            self.rows.clone().into_any_element()
        };
        div()
            .id("shelf")
            .key_context("Shelf")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &CloseShelf, window, cx| this.escape(window, cx)))
            // Tab reaches every control: rows, Copy for AI, Restart, the
            // clipboard buttons and the ▾, Open mdoc, Settings and Quit.
            .on_action(
                cx.listener(|_, _: &ui::NextControl, window, cx| {
                    ui::cycle(window, cx, None, false)
                }),
            )
            .on_action(cx.listener(|_, _: &ui::PreviousControl, window, cx| {
                ui::cycle(window, cx, None, true)
            }))
            .on_action(cx.listener(|this, _: &ShelfUp, window, cx| this.move_focus(-1, window, cx)))
            .on_action(
                cx.listener(|this, _: &ShelfDown, window, cx| this.move_focus(1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShelfCopyForAi, window, cx| {
                if let Some(index) = this.focused_row(window, cx) {
                    this.copy(index, window, cx)
                }
            }))
            .on_action(cx.listener(|this, row: &ShelfCopyRow, window, cx| {
                if !this.picking && row.0 >= 1 {
                    this.copy(row.0 - 1, window, cx)
                }
            }))
            // A drag released over the shelf lands here, not in another app.
            .on_drop(cx.listener(|this, _: &ExternalPaths, _, cx| this.dropped_on_shelf(cx)))
            .size_full()
            .overflow_y_scroll()
            .bg(surface(theme))
            .text_color(p.header_fg)
            .text_size(px(13.))
            .child(self.body(list, cx))
    }
}

impl ShelfView {
    /// Everything the shelf shows, in one column the popup measures.
    fn body(&self, list: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let update = self.update.clone();
        div()
            .relative()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .p(px(PADDING))
            .child(list)
            .child(self.divider())
            .when_some(update, |v, version| {
                v.child(self.action_row(
                    "shelf-restart",
                    text::restart_to_update(version),
                    cx,
                    |this, window, cx| this.restart(window, cx),
                ))
            })
            .child(self.clipboard_section(cx))
            .child(self.divider())
            .child(self.footer(cx))
            .when_some(self.fit.clone(), |body, fit| {
                body.child(
                    gpui::canvas(
                        move |bounds, window, cx| {
                            fit.measured(bounds.size.height.as_f32(), window, cx)
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            })
            .into_any_element()
    }
}
