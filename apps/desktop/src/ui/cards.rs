//! Cards: the empty-pane state card with its title, text and actions, the
//! model download's progress, and the content card's surface.

use crate::ui::{activity, progress_bar, style::Theme};
use gpui::{div, prelude::*, px};

/// A centred card for a pane that has no content yet. Callers add
/// the title, one sentence, actions and a footnote, in that order.
pub fn state_card(id: impl Into<gpui::ElementId>, theme: Theme) -> gpui::Stateful<gpui::Div> {
    let p = theme.pdf_style();
    div()
        .id(id)
        .w_full()
        .max_w(px(440.))
        .flex()
        .flex_col()
        .gap_2()
        .p_5()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(theme.sidebar_bg())
        .text_size(px(13.))
        .text_color(p.header_fg)
        .child(
            gpui::svg()
                .data(include_bytes!("../../resources/ui/document.svg"))
                .size(px(24.))
                .mb_1()
                .text_color(p.header_muted),
        )
}
pub fn card_title(text: impl Into<gpui::SharedString>) -> gpui::Div {
    div()
        .text_size(px(16.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(text.into())
}
pub fn card_text(text: impl Into<gpui::SharedString>, theme: Theme) -> gpui::Div {
    div()
        .text_size(px(13.))
        .text_color(theme.pdf_style().header_muted)
        .child(text.into())
}
pub fn card_note(text: impl Into<gpui::SharedString>, theme: Theme) -> gpui::Div {
    div()
        .pt_1()
        .text_size(px(11.))
        .text_color(theme.pdf_style().header_muted)
        .child(text.into())
}
/// Static model provenance next to the download size. It adds no Tab stop;
/// screen readers can read its named group and each prompt's existing actions.
pub fn model_attribution(
    id: impl Into<gpui::ElementId>,
    model: mdoc_store::settings::Model,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    let text = crate::app::models::attribution_label(model);
    div()
        .id(id)
        .role(gpui::Role::Group)
        .aria_label(format!("{}. {text}", model.name()))
        .min_w_0()
        .text_size(px(11.))
        .text_color(theme.pdf_style().header_muted)
        .child(text.clone())
        .when(cfg!(test), |v| {
            v.debug_selector(move || format!("{}: {text}", model.name()))
        })
}
/// A secondary explanation under a card's actions, e.g. language scope.
pub fn card_hint(text: impl Into<gpui::SharedString>, theme: Theme) -> gpui::Div {
    let p = theme.pdf_style();
    div()
        .flex()
        .gap_1()
        .text_size(px(12.))
        .text_color(p.header_muted)
        .child(div().flex_shrink_0().child("ⓘ"))
        .child(div().min_w_0().child(text.into()))
}
pub fn card_error(text: impl Into<gpui::SharedString>, theme: Theme) -> gpui::Div {
    div()
        .text_size(px(12.))
        .text_color(crate::ui::style::markdown_style(theme).alert_caution)
        .child(text.into())
}
pub fn card_actions() -> gpui::Div {
    div().flex().flex_wrap().items_center().gap_2().pt_2()
}

/// Download progress: total bundle bytes while downloading, otherwise the
/// current phase (Verifying, Checking runtime) without a percentage.
pub fn setup_progress(
    id: impl Into<gpui::ElementId>,
    state: &mdoc_import::model_download::State,
    cancelling: bool,
    theme: Theme,
) -> gpui::Div {
    let (received, total) = state.overall();
    let label = if cancelling {
        super::text::CANCELLING.to_owned()
    } else if state.downloading() {
        super::text::downloading((received * 100).checked_div(total).unwrap_or(0))
    } else if state.phase.is_empty() {
        super::text::PREPARING.to_owned()
    } else {
        format!("{}…", state.phase.trim_end_matches('…'))
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(activity(id, label, theme).flex_1())
                .when(state.downloading(), |v| {
                    v.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(theme.pdf_style().header_muted)
                            .child(super::text::megabytes_of(
                                received / 1_000_000,
                                mdoc_import::model_download::megabytes(total),
                            )),
                    )
                }),
        )
        .when(state.downloading(), |v| {
            v.child(progress_bar(received, total, theme))
        })
}

/// The content card's surface: rounded, on the window ground.
pub fn card_surface<E: Styled>(surface: E, theme: Theme) -> E {
    surface
        .rounded_lg()
        .overflow_hidden()
        .bg(theme.pdf_style().bg)
        .border_1()
        .border_color(theme.card_border())
        .shadow_sm()
}
