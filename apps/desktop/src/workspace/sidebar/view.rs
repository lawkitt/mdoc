//! The sidebar's view: its render with the rail, the panel, the top and the
//! foot, and the row helpers its tree and its drops share.
use super::state::{REVEAL_SLIDE, Reveal, SLIDE};
use super::text;
use crate::ui::{self, style, style::Theme};
use crate::workspace::Settings;
use crate::workspace::home::ShowHome;
use crate::workspace::palette::OpenPalette;
use crate::workspace::palette::key;
use crate::workspace::{New, RAIL_WIDTH, RESIZE_GRIP, ToggleSidebar, Workspace, palette};
use gpui::prelude::*;
use gpui::{Animation, AnimationExt, AnyElement, App, Context, Window, div, px};
use mdoc_import::import;
use mdoc_store::session_store::ThemeChoice;

/// The sidebar's controls in the window: the focus handles of its buttons,
/// rows and menus, its scroll, and the menus open over it.
pub(in crate::workspace) struct SidebarControls {
    pub(in crate::workspace) back_focus: gpui::FocusHandle,
    pub(in crate::workspace) forward_focus: gpui::FocusHandle,
    pub(in crate::workspace) reveal_focus: gpui::FocusHandle,
    pub(in crate::workspace) rail_toggle_focus: gpui::FocusHandle,
    pub(in crate::workspace) rail_new_focus: gpui::FocusHandle,
    pub(in crate::workspace) panel_toggle_focus: gpui::FocusHandle,
    pub(in crate::workspace) panel_new_focus: gpui::FocusHandle,
    /// Home's row in the collapsed rail and the expanded list: where F6
    /// enters the sidebar on Home.
    pub(in crate::workspace) rail_home_focus: gpui::FocusHandle,
    pub(in crate::workspace) panel_home_focus: gpui::FocusHandle,
    pub(in crate::workspace) sidebar_scroll: gpui::ScrollHandle,
    /// Each tree row's focus handle, by row.
    pub(in crate::workspace) tree_row_focus:
        std::cell::RefCell<std::collections::HashMap<super::tree::RowKey, gpui::FocusHandle>>,
    /// The recency switcher's focus, and where focus returns when it is
    /// cancelled.
    pub(in crate::workspace) switcher_focus: gpui::FocusHandle,
    pub(in crate::workspace) switcher_return: Option<gpui::FocusHandle>,
    /// A document row's menu: its document, and where it opened.
    pub(in crate::workspace) document_menu:
        Option<(mdoc_store::document_id::VaultId, gpui::Point<gpui::Pixels>)>,
    /// The document menu opened from Share, the ⋯ menu or the menu bar:
    /// moves only, no Delete, for the tab it was opened from; it closes
    /// once that tab no longer shows (#1312).
    pub(in crate::workspace) move_menu: Option<u64>,
    /// Its Project rows' focus, and its list's scroll, so Tab reveals the
    /// row it reaches.
    pub(in crate::workspace) move_row_focus:
        std::cell::RefCell<std::collections::HashMap<String, gpui::FocusHandle>>,
    pub(in crate::workspace) move_menu_scroll: gpui::ScrollHandle,
}

impl SidebarControls {
    /// The handles as the window first had them: no menu open.
    pub(in crate::workspace) fn new(cx: &mut App) -> Self {
        Self {
            back_focus: cx.focus_handle().tab_stop(true),
            forward_focus: cx.focus_handle().tab_stop(true),
            reveal_focus: cx.focus_handle(),
            rail_toggle_focus: cx.focus_handle().tab_stop(true),
            rail_new_focus: cx.focus_handle().tab_stop(true),
            panel_toggle_focus: cx.focus_handle().tab_stop(true),
            panel_new_focus: cx.focus_handle().tab_stop(true),
            rail_home_focus: cx.focus_handle().tab_stop(true),
            panel_home_focus: cx.focus_handle().tab_stop(true),
            sidebar_scroll: gpui::ScrollHandle::new(),
            tree_row_focus: Default::default(),
            switcher_focus: cx.focus_handle(),
            switcher_return: None,
            document_menu: None,
            move_menu: None,
            move_row_focus: Default::default(),
            move_menu_scroll: Default::default(),
        }
    }
}

/// A drag of the expanded sidebar's edge; the sidebar itself follows it.
#[derive(Clone)]
pub(in crate::workspace) struct SidebarResize;

impl Render for SidebarResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

/// A tab name that fits by losing its middle, never its extension, so
/// names with a shared start still differ. Drawn whole, so the
/// slack a cut leaves falls after the extension rather than before it.
pub(in crate::workspace) fn middle_truncated(name: &str) -> gpui::Div {
    let name = gpui::SharedString::from(name.to_string());
    div()
        .w_full()
        .relative()
        .overflow_hidden()
        // A line's height for the row; the canvas draws the name over it.
        .child("\u{200b}")
        .child(
            gpui::canvas(
                move |bounds, window, _| {
                    let style = window.text_style();
                    let size = style.font_size.to_pixels(window.rem_size());
                    let shape = |text: gpui::SharedString, window: &mut Window| {
                        let run = style.to_run(text.len());
                        window.text_system().shape_line(text, size, &[run], None)
                    };
                    let (stem, extension) = split_extension(&name);
                    let whole = shape(name.clone(), window);
                    if whole.width() <= bounds.size.width {
                        return whole;
                    }
                    let stem_line = shape(stem.to_string().into(), window);
                    let tail = shape(format!("\u{2026}{extension}").into(), window);
                    let room = (bounds.size.width - tail.width()) / px(1.);
                    let (front, back) = middle_cut(
                        stem,
                        |index| stem_line.x_for_index(index) / px(1.),
                        stem_line.width() / px(1.),
                        room,
                    );
                    let cut = format!("{}\u{2026}{}{extension}", &stem[..front], &stem[back..]);
                    shape(cut.into(), window)
                },
                |bounds, line, window, cx| {
                    line.paint(
                        bounds.origin,
                        bounds.size.height,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
                },
            )
            .absolute()
            .inset_0(),
        )
}

/// Where to cut `stem`, whose characters start at `x` and which is `width`
/// wide, to fit `room`: its start gets up to two thirds, its end whatever
/// the start left, at character boundaries. Returns the start's end and the
/// end's start.
pub(in crate::workspace) fn middle_cut(
    stem: &str,
    x: impl Fn(usize) -> f32,
    width: f32,
    room: f32,
) -> (usize, usize) {
    let room = room.max(0.);
    let front = stem
        .char_indices()
        .map(|(index, _)| index)
        .chain([stem.len()])
        .take_while(|&index| x(index) <= room * 2. / 3.)
        .last()
        .unwrap_or(0);
    let left = room - x(front);
    let back = stem[front..]
        .char_indices()
        .map(|(index, _)| front + index)
        .chain([stem.len()])
        .find(|&index| width - x(index) <= left)
        .unwrap_or(stem.len());
    (front, back)
}

/// A name's stem and its extension with the dot: a short final part
/// without spaces after a dot that does not start the name.
pub(in crate::workspace) fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot)
            if dot > 0
                && dot + 1 < name.len()
                && name.len() - dot <= 6
                && !name[dot..].contains(char::is_whitespace) =>
        {
            name.split_at(dot)
        }
        _ => (name, ""),
    }
}

/// A small mark on a sidebar tab for work done in it (OCR, pseudonymization).
pub(in crate::workspace) fn tab_badge(icon: ui::Icon, color: gpui::Hsla) -> gpui::Svg {
    gpui::svg()
        .data(icon.data())
        .size(px(12.))
        .flex_shrink_0()
        .text_color(color)
}

pub(in crate::workspace) fn sidebar_toggle(
    focus: &gpui::FocusHandle,
    expanded: bool,
    theme: Theme,
) -> ui::Control {
    let label = if expanded {
        text::COLLAPSE_SIDEBAR
    } else {
        text::EXPAND_SIDEBAR
    };
    // Quiet, like every other rail and title-bar icon.
    ui::header_icon_untipped("sidebar-toggle", label, ui::Icon::Sidebar, theme, false)
        .tooltip(style::tooltip(
            crate::commands::shortcuts::tip(label, &ToggleSidebar),
            theme,
        ))
        .size(px(32.))
        .track_focus(focus)
        .when(cfg!(test), |v| v.debug_selector(|| "sidebar-toggle".into()))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleSidebar), cx))
}

/// Whether a file drag holds anything mdoc opens (by extension, or a folder).
pub(in crate::workspace) fn openable_drag(paths: &gpui::ExternalPaths) -> bool {
    paths.paths().iter().any(|path| import::droppable(path))
}

/// The sidebar takes only drags it can open; others fall through untouched.
pub(in crate::workspace) fn can_drop_files(
    drag: &dyn std::any::Any,
    _: &mut Window,
    _: &mut App,
) -> bool {
    drag.downcast_ref::<gpui::ExternalPaths>()
        .is_none_or(openable_drag)
}

impl Workspace {
    pub(in crate::workspace) fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let expanded = self.sidebar.visible();
        let revealed = !expanded && self.sidebar.reveal() != Reveal::Hidden;
        let panel_width = self.sidebar.width();
        let menu_open = self.context_menu_open();
        let rows = self.tree_view(cx);
        let sliding = self.sidebar.sliding(std::time::Instant::now()) && !cx.reduce_motion();
        let collapsing = sliding && !expanded && !revealed;
        let mut panel = (expanded || revealed || collapsing).then(|| {
            div()
                .id("sidebar-panel")
                .w(px(panel_width - 1.))
                .h_full()
                .flex()
                .flex_col()
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, window, cx| {
                        let over = event.bounds.contains(&event.event.position);
                        let refused = over && !openable_drag(event.drag(cx));
                        let effect = this.sidebar.drag_over_list(over, refused);
                        this.carry_out_sidebar(effect, window, cx);
                    },
                ))
                .can_drop(can_drop_files)
                .on_drop(
                    cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                        this.drop_files(paths, window, cx)
                    }),
                )
                .child(
                    self.sidebar_top(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(sidebar_toggle(
                                &self.sidebar_controls.panel_toggle_focus,
                                expanded,
                                theme,
                            ))
                            .when(expanded || revealed, |v| {
                                v.child(self.navigation_buttons(cx))
                            })
                            .into_any_element(),
                    ),
                )
                .child(self.sidebar_nav(cx))
                // Above the scrolling list, so it shows however long it is.
                .when(self.sidebar.drop_refused(), |v| {
                    v.child(
                        div()
                            .when(cfg!(test), |v| v.debug_selector(|| "drop-refused".into()))
                            .px_4()
                            .py_1()
                            .text_color(style::markdown_style(theme).alert_caution)
                            .child(text::FILES_CANNOT_OPEN),
                    )
                })
                .child(
                    div()
                        .id("sidebar-tree")
                        .role(gpui::Role::Tree)
                        .aria_label(text::PROJECTS_AND_DOCUMENTS)
                        .track_scroll(&self.sidebar_controls.sidebar_scroll)
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px_2()
                        .pb_1()
                        .children(rows),
                )
                .child(self.sidebar_foot(cx))
        });
        let content = if expanded {
            // Expanding: the list is revealed inside its final width, so the
            // content beside it is laid out once, not on every frame.
            let list = div().size_full().children(panel.take());
            if sliding {
                div()
                    .size_full()
                    .child(
                        div()
                            .h_full()
                            .overflow_hidden()
                            .child(div().w(px(panel_width)).h_full().child(list))
                            .with_animation(
                                ("sidebar-open", self.sidebar.generation()),
                                Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
                                move |v, delta| {
                                    v.w(px(RAIL_WIDTH + (panel_width - RAIL_WIDTH) * delta))
                                },
                            ),
                    )
                    .into_any_element()
            } else {
                list.into_any_element()
            }
        } else {
            div()
                .w(px(RAIL_WIDTH - 1.))
                .h_full()
                .flex()
                .flex_col()
                // The rail starts below the title row, which holds the traffic
                // lights, the expand toggle and the document header.
                .child(ui::title_drag(
                    div()
                        .flex_shrink_0()
                        .h(px(style::CARD_MARGIN + style::HEADER_HEIGHT)),
                    self.title.armed.clone(),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(2.))
                        .pb_2()
                        .flex_shrink_0()
                        .child(
                            ui::header_icon(
                                "rail-search",
                                text::SEARCH_OR_RUN,
                                ui::Icon::Search,
                                theme,
                                false,
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(palette::OpenPalette), cx)
                            }),
                        )
                        .child(
                            ui::header_icon(
                                "rail-home",
                                text::HOME,
                                ui::Icon::Home,
                                theme,
                                self.showing_home(cx),
                            )
                            .track_focus(
                                &self
                                    .sidebar_controls
                                    .rail_home_focus
                                    .clone()
                                    .tab_stop(!revealed),
                            )
                            .when(cfg!(test), |v| v.debug_selector(|| "rail-home".into()))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_home(window, cx)),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .flex_1()
                        .min_h_0()
                        .child(
                            ui::header_icon_untipped(
                                "sidebar-new-collapsed",
                                text::NEW_DOCUMENT,
                                ui::Icon::Plus,
                                theme,
                                false,
                            )
                            .when(!menu_open, |v| {
                                v.tooltip(style::tooltip(
                                    crate::commands::shortcuts::tip(text::NEW_DOCUMENT, &New),
                                    theme,
                                ))
                            })
                            .track_focus(
                                &self
                                    .sidebar_controls
                                    .rail_new_focus
                                    .clone()
                                    .tab_stop(!revealed),
                            )
                            .when(cfg!(test), |v| {
                                v.debug_selector(|| "sidebar-new-collapsed".into())
                            })
                            .size(px(32.))
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(New), cx)),
                        ),
                )
                .child(self.rail_foot(cx))
                .into_any_element()
        };
        let width = |open: bool| if open { panel_width } else { RAIL_WIDTH };
        let mut sidebar = div()
            .id("sidebar")
            .track_focus(self.focus_regions.sidebar())
            .key_context("SidebarRegion")
            .relative()
            .w(px(width(expanded)))
            .flex_shrink_0()
            .h_full()
            .text_size(px(13.))
            // Keep the collapsed title controls in the sidebar's focus region
            // and before its list in Tab order. Deferred painting leaves them
            // above the adjacent content card without clipping to the rail.
            .when(!expanded && !revealed, |v| {
                v.child(gpui::deferred(
                    div()
                        .id("collapsed-title-controls")
                        .tab_index(-1)
                        .tab_group()
                        .tab_stop(false)
                        .absolute()
                        .left(px(style::TRAFFIC_LIGHT_INSET))
                        .top(px(style::CARD_MARGIN + (style::HEADER_HEIGHT - 32.) / 2.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(sidebar_toggle(
                            &self.sidebar_controls.rail_toggle_focus,
                            false,
                            theme,
                        ))
                        .child(self.navigation_buttons(cx)),
                ))
            })
            .child(div().size_full().overflow_hidden().child(content))
            // The expanded sidebar's edge drags its width; the
            // grip straddles the gap beside the content card.
            .when(expanded && !sliding, |v| {
                v.child(
                    div()
                        .id("sidebar-resize")
                        .when(cfg!(test), |v| v.debug_selector(|| "sidebar-resize".into()))
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(px(-RESIZE_GRIP))
                        .w(px(2. * RESIZE_GRIP))
                        .cursor_col_resize()
                        .on_drag(SidebarResize, |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        }),
                )
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<SidebarResize>, _, cx| {
                        if !this.sidebar.resizing() {
                            this.sidebar.begin_resize();
                        }
                        let width = (event.event.position.x - event.bounds.left()) / px(1.);
                        if this.sidebar.resize_to(width) {
                            cx.notify();
                        }
                    },
                ))
            })
            .when(!expanded, |v| {
                v.on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                    this.hover_sidebar(false, *hovered, window, cx)
                }))
                .on_drag_move(cx.listener(
                    |this, event: &gpui::DragMoveEvent<gpui::ExternalPaths>, window, cx| {
                        let over = event.bounds.contains(&event.event.position);
                        let effect = this.sidebar.drag_over_rail(over);
                        this.carry_out_sidebar(effect, window, cx);
                    },
                ))
                .can_drop(can_drop_files)
                .on_drop(cx.listener(
                    |this, paths: &gpui::ExternalPaths, window, cx| {
                        this.drop_files(paths, window, cx)
                    },
                ))
            });
        if revealed && let Some(panel) = panel.take() {
            let (from, to) = if self.sidebar.reveal() == Reveal::Shown {
                (RAIL_WIDTH, panel_width)
            } else {
                (panel_width, RAIL_WIDTH)
            };
            sidebar = sidebar.child(gpui::deferred(
                div()
                    .id("sidebar-reveal")
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(px(to))
                    .when(cfg!(test), |v| v.debug_selector(|| "sidebar-reveal".into()))
                    .key_context("UiPanel UiMenu")
                    .track_focus(&self.sidebar_controls.reveal_focus)
                    // Ahead of the editor in tab order, as the rail it covers.
                    .tab_index(-1)
                    .tab_group()
                    .tab_stop(false)
                    .occlude()
                    .overflow_hidden()
                    .bg(theme.sidebar_bg())
                    .rounded_r_lg()
                    .shadow_lg()
                    .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                        this.hover_sidebar(true, *hovered, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &ui::CloseMenu, window, cx| {
                        match this.sidebar.opener().cloned() {
                            Some(focus) => {
                                window.focus(&focus, cx);
                                this.sidebar.escape_to(focus);
                            }
                            None => this.focus_active(window, cx),
                        }
                        this.dismiss_reveal(window, cx);
                    }))
                    .child(panel)
                    .with_animation(
                        ("sidebar-reveal", self.sidebar.reveal_generation()),
                        Animation::new(REVEAL_SLIDE).with_easing(gpui::ease_out_quint()),
                        move |v, delta| v.w(px(from + (to - from) * delta)),
                    ),
            ));
        }
        // Collapsing: the content already has its final width; the full list
        // narrows over it to the rail, drawn above the card.
        if collapsing && let Some(list) = panel.take() {
            sidebar = sidebar.child(gpui::deferred(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .overflow_hidden()
                    .bg(theme.sidebar_bg())
                    .child(div().w(px(panel_width)).h_full().child(list))
                    .with_animation(
                        ("sidebar-close", self.sidebar.generation()),
                        Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
                        move |v, delta| v.w(px(panel_width - (panel_width - RAIL_WIDTH) * delta)),
                    ),
            ));
        }
        sidebar.into_any_element()
    }
}

impl Workspace {
    // ---- Sidebar ------------------------------------------------------------

    /// The sidebar's first row: room for the traffic lights, then the toggle.
    /// It moves the window like a title bar.
    pub(in crate::workspace) fn sidebar_top(&self, toggle: AnyElement) -> AnyElement {
        ui::title_drag(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(style::CARD_MARGIN + style::HEADER_HEIGHT))
                .pt(px(style::CARD_MARGIN))
                .pl(px(style::TRAFFIC_LIGHT_INSET))
                .pr_2(),
            self.title.armed.clone(),
        )
        .child(ui::no_drag(div().child(toggle)))
        .into_any_element()
    }
    /// Search ⌘K and Home under the top row.
    pub(in crate::workspace) fn sidebar_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let home = self.showing_home(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px_2()
            .pb_2()
            .flex_shrink_0()
            .child(
                crate::ui::focus_ring("sidebar-search", theme)
                    .when(cfg!(test), |v| v.debug_selector(|| "sidebar-search".into()))
                    .role(gpui::Role::Button)
                    .aria_label(text::SEARCH_OR_RUN)
                    .key_context("UiControl")
                    .tab_index(0)
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .px_2()
                    .mb_1()
                    .rounded_md()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.bg)
                    .text_color(p.header_muted)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(p.header_muted))
                    .child(
                        gpui::svg()
                            .data(ui::Icon::Search.data())
                            .size(px(14.))
                            .text_color(p.header_muted),
                    )
                    .child(div().flex_1().child(text::SEARCH))
                    .child(div().text_size(px(11.)).child(key(&OpenPalette)))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPalette), cx)),
            )
            .child(
                nav_row("sidebar-home", ui::Icon::Home, text::HOME, theme, home)
                    .track_focus(&self.sidebar_controls.panel_home_focus)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.header_muted)
                            .child(key(&ShowHome)),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.show_home(window, cx))),
            )
            .into_any_element()
    }
    /// A small section label, e.g. "Open" over tabs without a Project.
    pub(in crate::workspace) fn section_label(&self, label: &'static str) -> AnyElement {
        let p = self.theme.get().pdf_style();
        div()
            .px_2()
            .pt_3()
            .pb_1()
            .text_size(px(11.))
            .text_color(p.header_muted)
            .child(label)
            .into_any_element()
    }
    /// The update card and the Settings · theme · Help row.
    pub(in crate::workspace) fn sidebar_foot(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let theme_icon = match self.theme_choice {
            ThemeChoice::System => ui::Icon::ThemeSystem,
            ThemeChoice::Light => ui::Icon::Sun,
            ThemeChoice::Dark => ui::Icon::Moon,
        };
        let card = self.update_card(cx);
        // The card carries its own dot; Settings marks an update only without it.
        let dot = card.is_none() && crate::app::updater::state(cx).marks_settings();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .pb_2()
            .pt_1()
            .flex_shrink_0()
            .children(self.ai_indicator(cx))
            .children(card)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        nav_row("Settings", ui::Icon::Settings, text::SETTINGS, theme, false)
                            .track_focus(&self.settings_focus)
                            .flex_1()
                            .when(dot, |v| {
                                v.child(
                                    div()
                                        .when(cfg!(test), |v| {
                                            v.debug_selector(|| "update-dot".into())
                                        })
                                        .size(px(7.))
                                        .rounded_full()
                                        .bg(style::markdown_style(theme).alert_warning),
                                )
                            })
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(Settings), cx)
                            }),
                    )
                    .child(
                        ui::header_icon_untipped(
                            "theme-toggle",
                            style::theme_label(self.theme_choice),
                            theme_icon,
                            theme,
                            false,
                        )
                        // Clicking changes the choice under the pointer; the
                        // tooltip follows it instead of the value at hover.
                        .tooltip({
                            self.theme_tip.set(self.theme_choice);
                            let (choice, theme) = (self.theme_tip.clone(), self.theme.clone());
                            style::live_tooltip(move || {
                                (style::theme_label(choice.get()).into(), theme.get())
                            })
                        })
                        .when(cfg!(test), |v| v.debug_selector(|| "theme-toggle".into()))
                        .on_click(cx.listener(|this, _, window, cx| this.cycle_theme(window, cx))),
                    )
                    .child(
                        ui::header_icon("sidebar-help", "Help ↗", ui::Icon::Help, theme, false)
                            .on_click(|_, _, cx| cx.open_url(crate::commands::menus::HELP_URL)),
                    ),
            )
            .child(
                div()
                    .h(px(1.))
                    .bg(gpui::transparent_black())
                    .text_color(p.header_muted),
            )
            .into_any_element()
    }
    /// The collapsed rail's foot: Settings only, an update shown as a dot on
    /// it. Theme and Help live in the open sidebar.
    pub(in crate::workspace) fn rail_foot(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let update = crate::app::updater::state(cx).marks_settings();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .pt_1()
            .pb_3()
            .flex_shrink_0()
            .children(self.ai_rail_icon(cx))
            .child(
                div()
                    .relative()
                    .child(
                        ui::header_icon(
                            "rail-settings",
                            if update {
                                text::SETTINGS_UPDATE_AVAILABLE
                            } else {
                                text::SETTINGS
                            },
                            ui::Icon::Settings,
                            theme,
                            false,
                        )
                        .when(cfg!(test), |v| v.debug_selector(|| "rail-settings".into()))
                        .size(px(32.))
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(Settings), cx)),
                    )
                    .when(update, |v| {
                        v.child(
                            div()
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "rail-update-dot".into())
                                })
                                .absolute()
                                .top(px(6.))
                                .right(px(6.))
                                .size(px(6.))
                                .rounded_full()
                                .bg(style::markdown_style(theme).alert_warning),
                        )
                    }),
            )
            .into_any_element()
    }
}

pub(super) fn nav_row(
    id: &'static str,
    icon: ui::Icon,
    label: &'static str,
    theme: Theme,
    selected: bool,
) -> ui::Control {
    let p = theme.pdf_style();
    crate::ui::focus_ring(id, theme)
        .when(cfg!(test), |v| v.debug_selector(move || id.into()))
        .role(gpui::Role::Button)
        .aria_label(label)
        .key_context("UiControl")
        .tab_index(0)
        .flex()
        .items_center()
        .gap_2()
        .h(px(30.))
        .px_2()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |v| v.bg(theme.sidebar_selected()))
        .hover(move |s| s.bg(p.placeholder_bg))
        .child(
            gpui::svg()
                .data(icon.data())
                .size(px(15.))
                .flex_shrink_0()
                .text_color(if selected {
                    p.header_fg
                } else {
                    p.header_muted
                }),
        )
        .child(div().flex_1().child(label))
}
