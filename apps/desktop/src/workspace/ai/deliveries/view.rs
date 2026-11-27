//! Deliveries' view: the AI card's badge and the rail's. A click opens the
//! newest through the owner.
use crate::ui;
use crate::workspace::Workspace;
use crate::workspace::ai::text;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

impl Workspace {
    /// The AI card's badge: how many things arrived and are not opened
    /// yet; a click opens the newest.
    pub(in crate::workspace) fn ai_badge(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let count = self.ai.unseen().len();
        if count == 0 {
            return None;
        }
        let theme = self.theme.get();
        Some(
            // A control on the accent: its own hover, not the grey one.
            ui::focus_ring("ai-badge", theme)
                .when(cfg!(test), |v| v.debug_selector(|| "ai-badge".into()))
                .role(gpui::Role::Button)
                .aria_label(text::new_from_ai_apps(count))
                .key_context("UiControl")
                .tab_index(0)
                .tab_stop(true)
                .cursor_pointer()
                .hover(|s| s.opacity(0.85))
                .flex_shrink_0()
                .px_2()
                .py(px(1.))
                .rounded_full()
                .bg(theme.search_accent())
                .text_color(theme.badge_fg())
                .text_size(px(10.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(text::new_count(count))
                .on_click(cx.listener(|this, _, window, cx| this.open_newest(window, cx)))
                .into_any_element(),
        )
    }

    /// The collapsed rail's count of what arrived, over the AI icon.
    pub(in crate::workspace) fn ai_rail_badge(&self) -> Option<AnyElement> {
        let count = self.ai.unseen().len();
        if count == 0 {
            return None;
        }
        let theme = self.theme.get();
        Some(
            div()
                .when(cfg!(test), |v| v.debug_selector(|| "rail-ai-badge".into()))
                .absolute()
                .top(px(1.))
                .right(px(0.))
                .min_w(px(14.))
                .h(px(14.))
                .px(px(3.))
                .rounded_full()
                .bg(theme.search_accent())
                .text_color(theme.badge_fg())
                .text_size(px(10.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .flex()
                .items_center()
                .justify_center()
                .child(SharedString::from(count.to_string()))
                .into_any_element(),
        )
    }
}
