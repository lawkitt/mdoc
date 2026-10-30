//! The selection pill's category menu: "Replace · Person ▾" adds the
//! selection in a picked category (ADR 0024).
use crate::{document_view::DocumentView, pii::ui::PiiAddCandidateAs};
use gpui::{
    AnyElement, Context, Focusable, SharedString, Window, anchored, deferred, point, prelude::*, px,
};
use mdoc_pii::Category;

/// Room the menu needs above the pill to open upward.
const MENU_HEIGHT: f32 = 340.;

impl DocumentView {
    /// ⇧⌘⌥P, or the pill's ▾: open the category menu for the selection with
    /// focus on the category Replace would use; again closes it.
    pub(crate) fn add_pii_candidate_as(
        &mut self,
        _: &PiiAddCandidateAs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_selection_menu(window, cx);
    }
    pub(crate) fn toggle_selection_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_selection_menu(window, cx) {
            return;
        }
        let Some(Ok(range)) = self.manual_target(cx) else {
            return;
        };
        let editor = self.editor.read(cx);
        let guess = self.manual_guess(editor.text(), range);
        self.pii.selection_menu = Some(editor.selection());
        let row = self.selection_menu_row(guess, cx);
        window.focus(&row, cx);
        self.sync_selection_action(cx);
        cx.notify();
    }
    /// Close the menu, handing focus back to the editor if the menu held it.
    fn close_selection_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.pii.selection_menu.take().is_none() {
            return false;
        }
        if self.pii.selection_menu_focus.contains_focused(window, cx) {
            let editor = self.editor.read(cx).focus_handle(cx);
            window.focus(&editor, cx);
        }
        self.sync_selection_action(cx);
        cx.notify();
        true
    }
    fn selection_menu_row(&self, category: Category, cx: &gpui::App) -> gpui::FocusHandle {
        self.pii.popup_control_focus(selection_row_id(category), cx)
    }
    /// The open menu under (or, with room, over) the pill's "Person ▾" part:
    /// each category with the alias it would give, the guess ticked. It holds
    /// focus like the mention bar's category menu (ADR 0062).
    pub(in crate::pii::ui) fn selection_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.pii.selection_menu.as_ref()?;
        let editor = self.editor.read(cx);
        let Some(anchor) = editor.selection_choice_bounds() else {
            // The pill is painted in this frame; place the menu in the next.
            window.request_animation_frame();
            return None;
        };
        let Some(Ok(range)) = self.manual_target(cx) else {
            return None;
        };
        let source = editor.text();
        let original = source[range.clone()].to_owned();
        let guess = self.manual_guess(source, range);
        let review = &self.pii.review;
        let enabled = !self.pii.scanning();
        let back = editor.focus_handle(cx);
        let menu = crate::ui::panel("selection-category-menu", self.theme.get())
            .when(cfg!(test), |v| {
                v.debug_selector(|| "selection-category-menu".into())
            })
            .w(px(240.))
            .p_1()
            .flex()
            .flex_col()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let menu = self
            .focus_menu(
                menu,
                &self.pii.selection_menu_focus,
                &back,
                |this, window, cx| this.close_selection_menu(window, cx),
                cx,
            )
            .children(Category::ALL.into_iter().map(|category| {
                self.menu_row(
                    selection_row_id(category),
                    category.label(),
                    review.manual_alias(&original, category),
                    Some(category == guess),
                    enabled,
                    cx,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.add_manual_candidate(Some(category), window, cx);
                }))
            }));
        let (corner, at) = if anchor.top() > px(MENU_HEIGHT) {
            (
                gpui::Anchor::BottomLeft,
                point(anchor.left(), anchor.top() - px(4.)),
            )
        } else {
            (
                gpui::Anchor::TopLeft,
                point(anchor.left(), anchor.bottom() + px(4.)),
            )
        };
        Some(
            deferred(
                anchored()
                    .anchor(corner)
                    .position(at)
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}

fn selection_row_id(category: Category) -> SharedString {
    format!("selection-category-{}", category.token()).into()
}
