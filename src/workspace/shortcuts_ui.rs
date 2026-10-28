//! The keyboard shortcuts sheet (ADR 0073): a read-only modal over the
//! window listing the one shortcut table by context, with this platform's
//! keys only. A filter field narrows it; Esc closes it and focus goes back
//! where it was.
use super::Workspace;
use crate::{
    markdown_search::{SearchInput, SearchInputEvent},
    shortcuts::{self, Group},
    ui,
};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, Focusable, KeyDownEvent, ScrollHandle, Subscription,
    Window, actions, div, point, prelude::*, px,
};

actions!(mdoc_shortcuts, [ShowShortcuts, CloseShortcuts]);

/// The sheet's key context.
pub(crate) const SHEET: &str = "ShortcutsSheet";
/// The filter field's key context, one of the text fields'.
const FILTER: &str = "ShortcutsFilter";
/// How far one arrow press scrolls the list.
const LINE: f32 = 28.;

pub(super) struct ShortcutsSheet {
    filter: Entity<SearchInput>,
    /// The sheet itself: keeps Tab inside.
    focus: FocusHandle,
    /// The list, a Tab stop that scrolls with the arrow keys.
    list: FocusHandle,
    scroll: ScrollHandle,
    return_focus: Option<FocusHandle>,
    _changes: Subscription,
}

/// One shown row: its label and its keys as this platform writes them.
pub(crate) struct Row {
    pub label: &'static str,
    pub keys: Vec<String>,
}

/// The rows the sheet shows for `query`, by group in the sheet's order.
/// Every word of the query must appear in the row's label, its group's
/// title or one of its keys, ignoring case.
pub(crate) fn visible(query: &str) -> Vec<(Group, Vec<Row>)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    Group::ALL
        .into_iter()
        .filter_map(|group| {
            let rows: Vec<Row> = shortcuts::rows()
                .filter(|row| row.group == group)
                .map(|row| Row {
                    label: row.label,
                    keys: row.key_labels(),
                })
                .filter(|row| {
                    let text = format!("{} {} {}", row.label, group.title(), row.keys.join(" "))
                        .to_lowercase();
                    words.iter().all(|word| text.contains(word.as_str()))
                })
                .collect();
            (!rows.is_empty()).then_some((group, rows))
        })
        .collect()
}

impl Workspace {
    /// ⌘/ (Ctrl+/), `?` outside text fields, the palette's and the Help
    /// menu's Keyboard Shortcuts: open the sheet, or close it when open.
    pub(super) fn toggle_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shell.shortcuts.is_some() {
            self.close_shortcuts(window, cx);
            return;
        }
        let filter = cx.new(|cx| {
            SearchInput::new(cx)
                .with_key_context(FILTER)
                .with_placeholder("Filter shortcuts")
        });
        let changes = cx.subscribe(&filter, |this, _, _: &SearchInputEvent, cx| {
            if let Some(sheet) = &this.shell.shortcuts {
                sheet.scroll.set_offset(point(px(0.), px(0.)));
            }
            cx.notify();
        });
        let return_focus = window.focused(cx);
        window.focus(&filter.read(cx).focus_handle(cx), cx);
        self.shell.shortcuts = Some(ShortcutsSheet {
            filter,
            focus: cx.focus_handle(),
            list: cx.focus_handle().tab_stop(true),
            scroll: ScrollHandle::new(),
            return_focus,
            _changes: changes,
        });
        cx.notify();
    }

    pub(super) fn close_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(sheet) = self.shell.shortcuts.take() {
            match sheet.return_focus {
                Some(focus) => window.focus(&focus, cx),
                None => self.focus_active(window, cx),
            }
        }
        cx.notify();
    }

    /// The list scrolls with ↑ ↓, Page Up / Down, Home and End.
    fn scroll_shortcuts(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(sheet) = &self.shell.shortcuts else {
            return;
        };
        if event.keystroke.modifiers.modified() {
            return;
        }
        let page = (sheet.scroll.bounds().size.height - px(LINE)).max(px(LINE));
        let max = sheet.scroll.max_offset().y;
        let y = -sheet.scroll.offset().y;
        let y = match event.keystroke.key.as_str() {
            "up" => y - px(LINE),
            "down" => y + px(LINE),
            "pageup" => y - page,
            "pagedown" => y + page,
            "home" => px(0.),
            "end" => max,
            _ => return,
        };
        sheet
            .scroll
            .set_offset(point(px(0.), -y.clamp(px(0.), max.max(px(0.)))));
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn shortcuts_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.shell.shortcuts.as_ref()?;
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let groups = visible(sheet.filter.read(cx).value());
        let mut list = div()
            .id("shortcuts-list")
            .when(cfg!(test), |v| v.debug_selector(|| "shortcuts-list".into()))
            .role(gpui::Role::List)
            .aria_label("Shortcuts")
            .track_focus(&sheet.list)
            .track_scroll(&sheet.scroll)
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .rounded_sm()
            .border_1()
            .border_color(gpui::transparent_black())
            .focus_visible(move |s| s.border_color(theme.search_accent()))
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
                let keys = row.keys.join(", ");
                index += 1;
                list = list.child(
                    div()
                        .id(("shortcut-row", index))
                        .when(cfg!(test), |v| {
                            v.debug_selector(move || format!("shortcut-row-{index}"))
                        })
                        .role(gpui::Role::ListItem)
                        .aria_label(format!("{}, {keys}", row.label))
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
                    .child("No shortcut matches."),
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
                    .bg(gpui::hsla(0., 0., 0., 0.25))
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
                            .aria_label("Keyboard Shortcuts")
                            .key_context(format!("{SHEET} UiPanel").as_str())
                            .track_focus(&sheet.focus)
                            .tab_group()
                            .tab_stop(false)
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
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
                            .child(ui::card_title("Keyboard Shortcuts"))
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
