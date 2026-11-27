//! The palette's view: the search field over its rows, by section, with
//! each row's detail and key.

use super::text;
use super::{Command, PALETTE, PageAction, PaletteClose, PaletteConfirm, PaletteDown, PaletteUp};
use crate::ui;
use crate::workspace::Workspace;
use gpui::AnyElement;
use gpui::Context;
use gpui::div;
use gpui::prelude::*;
use gpui::px;

impl Workspace {
    pub(in crate::workspace) fn palette_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let palette = self.shell.palette.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let rows = self.palette_rows(cx);
        let selected = palette.selected.min(rows.len().saturating_sub(1));
        let mut list = div()
            .id("palette-list")
            .track_scroll(&palette.scroll)
            .max_h(px(470.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px_1()
            .pb_1();
        let mut section = "";
        for (i, row) in rows.iter().enumerate() {
            if row.section != section {
                section = row.section;
                list = list.child(
                    div()
                        .px_2()
                        .pt_2()
                        .pb_1()
                        .text_size(px(11.))
                        .text_color(p.header_muted)
                        .child(section),
                );
            }
            let icon = match &row.command {
                Command::NewTab | Command::PasteText | Command::NewProject => ui::Icon::Plus,
                Command::OpenFiles => ui::Icon::Open,
                Command::Document(_) => ui::Icon::Source,
                Command::Heading(_) => ui::Icon::ChevronRight,
                Command::Home => ui::Icon::Home,
                Command::Settings | Command::SettingsCategory(_) => ui::Icon::Settings,
                Command::Shortcuts => ui::Icon::Help,
                Command::Theme => ui::Icon::ThemeSystem,
                Command::RestoreReply => ui::Icon::Reply,
                Command::CopyForAi | Command::CopyMarkdown => ui::Icon::Copy,
                Command::Save | Command::ExportMarkdown => ui::Icon::Save,
                Command::Close | Command::CancelConversion => ui::Icon::Close,
                Command::BeforeAfter => ui::Icon::PanelRight,
                Command::DecisionHistory => ui::Icon::Clock,
                Command::Notes => ui::Icon::PanelRight,
                Command::SourceReply => ui::Icon::Reply,
                Command::ExpandIdentities => ui::Icon::ExpandAll,
                Command::CollapseIdentities => ui::Icon::CollapseAll,
                Command::ToggleSidebar => ui::Icon::Sidebar,
                Command::Pseudonymize | Command::PseudonymizeClipboard => ui::Icon::Anonymous,
                Command::Project(_) | Command::OpenProject => ui::Icon::Sidebar,
                Command::Page(PageAction::CopyReply) => ui::Icon::Copy,
                Command::Page(_) => ui::Icon::Sidebar,
            };
            list = list.child(
                div()
                    .id(("palette-row", i))
                    .when(cfg!(test), |v| {
                        v.debug_selector(move || format!("palette-row-{i}"))
                    })
                    .role(gpui::Role::MenuItem)
                    .aria_label(row.label.clone())
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(i == selected, |v| v.bg(p.placeholder_bg))
                    .hover(move |s| s.bg(p.placeholder_bg))
                    .child(
                        gpui::svg()
                            .data(icon.data())
                            .size(px(15.))
                            .text_color(p.header_muted),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(row.label.clone()))
                    .when_some(row.detail.clone(), |v, d| {
                        v.child(div().text_size(px(11.)).text_color(p.header_muted).child(d))
                    })
                    .when_some(row.command.shortcut(), |v, s| {
                        v.child(div().text_size(px(11.)).text_color(p.header_muted).child(s))
                    })
                    .on_click({
                        let command = row.command.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.palette_run(command.clone(), window, cx)
                        })
                    }),
            );
        }
        if rows.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_4()
                    .text_color(p.header_muted)
                    .child(text::NOTHING_MATCHES),
            );
        }
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .justify_center()
                    // The palette hugs its rows instead of stretching down.
                    .items_start()
                    .pt(px(72.))
                    .bg(gpui::hsla(0., 0., 0., 0.18))
                    .occlude()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
                    )
                    .child(
                        ui::panel("command-palette", theme)
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "command-palette".into())
                            })
                            .key_context(PALETTE)
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_action(
                                cx.listener(|this, _: &PaletteUp, _, cx| this.palette_step(-1, cx)),
                            )
                            .on_action(
                                cx.listener(|this, _: &PaletteDown, _, cx| {
                                    this.palette_step(1, cx)
                                }),
                            )
                            .on_action(cx.listener(|this, _: &PaletteConfirm, window, cx| {
                                this.palette_confirm(None, window, cx)
                            }))
                            .on_action(cx.listener(|this, _: &PaletteClose, window, cx| {
                                this.close_palette(window, cx)
                            }))
                            .w(px(560.))
                            .flex()
                            .flex_col()
                            .rounded_xl()
                            .shadow_lg()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .h(px(44.))
                                    .flex_shrink_0()
                                    .border_b_1()
                                    .border_color(p.border)
                                    .child(
                                        gpui::svg()
                                            .data(ui::Icon::Search.data())
                                            .size(px(15.))
                                            .text_color(p.header_muted),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(14.))
                                            .child(palette.input.clone()),
                                    ),
                            )
                            .child(list),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
