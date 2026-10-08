//! PII toolbar and popup routing. State stays in the parent controller.
use super::{Popup, ReviewUi};
use crate::{Workspace, ui};
use gpui::{AnyElement, App, Context, Window, div, prelude::*};

impl ReviewUi {
    pub(super) fn reveal_popup_control(
        &self,
        id: impl Into<gpui::ElementId>,
        control: gpui::Stateful<gpui::Div>,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let focus = self
            .popup_controls
            .borrow_mut()
            .entry(id.into())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        ui::reveal_focus(
            control.track_focus(&focus),
            focus,
            self.popup_scroll.clone(),
        )
    }
}

impl Workspace {
    pub(crate) fn pii_toolbar_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let enabled = self.can_copy_markdown();
        div()
            .id("pii-toolbar")
            .flex()
            .items_center()
            .child(
                ui::icon_control(
                    "Pseudonymize",
                    "Pseudonymize",
                    ui::Icon::Anonymous,
                    theme,
                    enabled,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    if enabled {
                        this.toggle_pii_review(window, cx);
                    }
                })),
            )
            .into_any_element()
    }
    pub(crate) fn pii_popup(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Popup::Choose { ids, selected } = self.pii.popup.as_ref()? {
            return Some(self.annotation_chooser(ids.clone(), *selected, cx));
        }
        self.replacement_popup(window, cx)
    }
}
