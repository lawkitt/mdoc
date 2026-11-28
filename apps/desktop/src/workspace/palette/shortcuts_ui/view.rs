//! The shortcuts sheet's view: the modal, its filter field and the list
//! by group.

use super::{CloseShortcuts, LINE, SHEET, visible};
use crate::ui;
use crate::workspace::Workspace;
use gpui::{AnyElement, Context, Window, div, prelude::*, px};

impl Workspace {
    pub(in crate::workspace) fn shortcuts_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.shell.shortcuts.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let groups = visible(sheet.filter.read(cx).value());
        let mut list = ui::focus_ring("shortcuts-list", theme)
            .when(cfg!(test), |v| v.debug_selector(|| "shortcuts-list".into()))
            .role(gpui::Role::List)
            .aria_label(super::super::text::SHORTCUTS)
            .track_focus(&sheet.list)
            .track_scroll(&sheet.scroll)
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .rounded_sm()
            .border_1()
            .border_color(gpui::transparent_black())
            .on_key_down(cx.listener(|this, event, _, cx| this.scroll_shortcuts(event, cx)))
            .flex()
            .flex_col()
            .pb_2();
        let mut index: usize = 0;
        for (group, rows) in &groups {
            list = list.child(
                div()
                    .id(("shortcuts-group", *group as usize))
                    .role(gpui::Role::Heading)
                    .aria_level(2)
                    .aria_label(group.title())
                    .px_2()
                    .pt_3()
                    .pb_1()
                    .text_size(px(11.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(p.header_muted)
                    .child(group.title()),
            );
            for row in rows {
                index += 1;
                list = list.child(
                    div()
                        .id(("shortcut-row", index))
                        .when(cfg!(test), |v| {
                            v.debug_selector(move || format!("shortcut-row-{index}"))
                        })
                        .role(gpui::Role::ListItem)
                        .aria_label(super::super::text::shortcut_row(row.label, &row.keys))
                        .flex()
                        .items_center()
                        .gap_4()
                        .min_h(px(LINE))
                        .px_2()
                        .rounded_sm()
                        .hover(move |s| s.bg(p.placeholder_bg))
                        .child(div().flex_1().min_w_0().child(row.label))
                        .child(
                            div().flex().flex_shrink_0().gap_1().children(
                                row.keys
                                    .iter()
                                    .map(|key| ui::key_hint(key.clone()).text_color(p.header_fg)),
                            ),
                        ),
                );
            }
        }
        if groups.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_4()
                    .text_color(p.header_muted)
                    .child(super::super::text::NO_SHORTCUT_MATCHES),
            );
        }
        let height = (window.viewport_size().height - px(144.)).max(px(240.));
        Some(
            gpui::deferred(
                div()
                    .id("shortcuts-overlay")
                    .absolute()
                    .inset_0()
                    .flex()
                    .justify_center()
                    .items_start()
                    .pt(px(72.))
                    .bg(theme.scrim())
                    // A modal: nothing behind it takes the pointer.
                    .occlude()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.close_shortcuts(window, cx)),
                    )
                    .child(
                        ui::panel("shortcuts-sheet", theme)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "shortcuts-sheet".into())
                            })
                            .role(gpui::Role::Dialog)
                            .aria_label(super::super::text::KEYBOARD_SHORTCUTS_TITLE)
                            .key_context(format!("{SHEET} UiPanel").as_str())
                            .track_focus(&sheet.focus)
                            .tab_group()
                            .tab_stop(false)
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            // Other modal surfaces would take focus they
                            // cannot give back once the sheet is gone.
                            .on_action(|_: &crate::workspace::OpenPalette, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_action(|_: &crate::workspace::Settings, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_action(|_: &crate::workspace::OpenProject, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_action(|_: &crate::workspace::NewProject, _, cx| {
                                cx.stop_propagation()
                            })
                            // Taking focus above Settings must not bypass its
                            // restrictions on document commands.
                            .when(
                                self.settings.read(cx).open,
                                crate::settings::block_document_commands,
                            )
                            .on_action(cx.listener(|this, _: &CloseShortcuts, window, cx| {
                                this.close_shortcuts(window, cx)
                            }))
                            .on_action(cx.listener(|this, _: &ui::NextControl, window, cx| {
                                if let Some(sheet) = &this.shell.shortcuts {
                                    ui::cycle(window, cx, Some(&sheet.focus), false);
                                }
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(|this, _: &ui::PreviousControl, window, cx| {
                                if let Some(sheet) = &this.shell.shortcuts {
                                    ui::cycle(window, cx, Some(&sheet.focus), true);
                                }
                                cx.stop_propagation();
                            }))
                            .w(px(560.))
                            .max_w_full()
                            .h(height)
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(ui::card_title(super::super::text::KEYBOARD_SHORTCUTS_TITLE))
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(p.border)
                                    .child(sheet.filter.clone()),
                            )
                            .child(list),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
