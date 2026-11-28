//! How the before/after view draws: the floating view switcher and the
//! columns with their labels.
use super::{CompareMode, FLOAT_TOP, LABEL_HEIGHT, NARROW, SWITCHER_FADE, text};
use crate::{document::DocumentView, ui};
use gpui::{AnyElement, Context, Entity, Pixels, Window, div, prelude::*, px};

impl DocumentView {
    /// The floating pill at the bottom of the document: zoom
    /// always; Edit · Original · Pseudonymized · Side by side once aliases are
    /// applied, and Reply while a reply exists.
    pub(crate) fn view_switcher(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.lifecycle.source_only() {
            return None;
        }
        let views = self.has_applied_aliases();
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let current = self.compare.as_ref().map(|state| state.mode);
        let segment = |id: &'static str, label: &'static str, mode: Option<CompareMode>| {
            let selected = current == mode;
            ui::control(id, label, theme, true)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .aria_toggled(if selected {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .px_3()
                .text_size(px(12.))
                .when(selected, |v| {
                    v.bg(palette.placeholder_bg).text_color(palette.header_fg)
                })
                .when(!selected, |v| v.text_color(palette.header_muted))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.set_view_mode(mode, window, cx)),
                )
        };
        let zoom_step = |id: &'static str, label: &'static str, name: &'static str, delta: f32| {
            ui::control(id, label, theme, true)
                .aria_label(name)
                .when(cfg!(test), move |v| v.debug_selector(move || id.into()))
                .px_2()
                .text_size(px(13.))
                .text_color(palette.header_muted)
                .on_click(cx.listener(move |this, _, _, cx| this.set_zoom(this.zoom + delta, cx)))
        };
        let percent = format!("{}%", (self.zoom * 100.).round() as i32);
        // Fade only on a change of visibility; the first frame and Reduce
        // Motion jump straight to the resting state. Opacity is computed here
        // rather than with a keyed animation so the pill's element ids, and a
        // click in progress, survive the change.
        let progress = match self.switcher.changed_at {
            Some(at) if !cx.reduce_motion() => {
                (at.elapsed().as_secs_f32() / SWITCHER_FADE.as_secs_f32()).min(1.)
            }
            _ => 1.,
        };
        if progress < 1. {
            window.request_animation_frame();
        }
        // Keyboard reach must never land on a faded control,
        // including during the idle/fade timer.
        let opacity = if self.switcher_region.contains_focused(window, cx) {
            1.
        } else if self.switcher.shown() {
            progress
        } else {
            1. - progress
        };
        Some(
            div()
                .absolute()
                .bottom(px(16.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("view-switcher")
                        .track_focus(&self.switcher_region)
                        .key_context("ToolbarRegion")
                        .when(cfg!(test), |v| v.debug_selector(|| "view-switcher".into()))
                        .role(gpui::Role::Group)
                        .aria_label(text::VIEW)
                        .flex()
                        .items_center()
                        .gap_1()
                        .p_1()
                        .rounded_xl()
                        .bg(palette.bg)
                        .border_1()
                        .border_color(palette.border)
                        .shadow_md()
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            let shown = this.switcher.shown();
                            this.switcher.hovered = *hovered;
                            this.switcher.settle(shown, cx);
                            if !*hovered {
                                this.wake_switcher(cx);
                            }
                        }))
                        .when(views || self.reply.exists(), |v| {
                            v.child(segment("view-edit", text::EDIT, None))
                        })
                        .when(views, |v| {
                            v.child(segment(
                                "view-original",
                                text::ORIGINAL,
                                Some(CompareMode::Original),
                            ))
                            .child(segment(
                                "view-pseudonymized",
                                text::PSEUDONYMIZED,
                                Some(CompareMode::Pseudonymized),
                            ))
                            .child(segment(
                                "view-side",
                                text::SIDE_BY_SIDE,
                                Some(CompareMode::Side),
                            ))
                        })
                        .when(self.reply.exists(), |v| {
                            v.child(segment("view-reply", text::REPLY, Some(CompareMode::Reply)))
                        })
                        .when(views || self.reply.exists(), |v| {
                            v.child(div().w(px(1.)).h(px(16.)).mx_1().bg(palette.border))
                        })
                        .child(zoom_step(
                            "zoom-out",
                            "−",
                            text::ZOOM_OUT,
                            -crate::document::ZOOM_STEP,
                        ))
                        .child(
                            ui::control("zoom-reset", percent, theme, true)
                                .when(cfg!(test), |v| v.debug_selector(|| "zoom-reset".into()))
                                .aria_label(text::RESET_ZOOM)
                                .min_w(px(48.))
                                .flex()
                                .justify_center()
                                .text_size(px(12.))
                                .on_click(cx.listener(|this, _, _, cx| this.set_zoom(1., cx))),
                        )
                        .child(zoom_step(
                            "zoom-in",
                            "+",
                            text::ZOOM_IN,
                            crate::document::ZOOM_STEP,
                        ))
                        .opacity(opacity),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn compare_view(&self, width: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.compare.as_ref()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        // A single-column mode shows one column full width; Side by side
        // shows one at a time only in a narrow window.
        let narrow = width < NARROW || state.mode != CompareMode::Side;
        // Scroll together: whichever pane moved since the last frame leads,
        // by the same fraction of its height (lines pair up, wrapping differs).
        let (left, right) = (state.left.offset().y, state.right.offset().y);
        let (last_left, last_right) = state.last.get();
        let follow = |from: &gpui::ScrollHandle, to: &gpui::ScrollHandle, y: Pixels| {
            let max = from.max_offset().y;
            let fraction = if max > px(0.) {
                f32::from(-y) / f32::from(max)
            } else {
                0.
            };
            let mut offset = to.offset();
            offset.y = -(to.max_offset().y * fraction.clamp(0., 1.));
            to.set_offset(offset);
        };
        if left != last_left {
            follow(&state.left, &state.right, left);
        } else if right != last_right {
            follow(&state.right, &state.left, right);
        }
        state
            .last
            .set((state.left.offset().y, state.right.offset().y));
        // A quiet floating label names each column; it dims once the column
        // scrolls under it. In a narrow window it switches columns.
        let switchable = narrow && state.mode == CompareMode::Side;
        // The label centers on Close's line, which floats at the same top (U25).
        let label_top = FLOAT_TOP + (ui::FLOATING_BUTTON_HEIGHT - LABEL_HEIGHT) / 2.;
        let label = |id: &'static str, text: &'static str, scroll: &gpui::ScrollHandle| {
            let scrolled = scroll.offset().y < px(-8.);
            div()
                .id(id)
                .when(cfg!(test), |v| v.debug_selector(move || id.into()))
                .absolute()
                .top(px(label_top))
                .left(px(16.))
                .h(px(LABEL_HEIGHT))
                .flex()
                .items_center()
                .px_2()
                .rounded_md()
                .bg(palette.bg)
                .border_1()
                .border_color(palette.border)
                .text_size(px(11.))
                .text_color(palette.header_muted)
                .when(scrolled, |v| v.opacity(0.35))
                .hover(|s| s.opacity(1.))
                .child(if switchable {
                    format!("{text}  ⇄")
                } else {
                    text.into()
                })
                .when(switchable, |v| {
                    v.cursor_pointer()
                        .tooltip(crate::ui::style::tooltip(text::SWITCH_COLUMN.into(), theme))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(state) = this.compare.as_mut() {
                                state.original_only = !state.original_only;
                            }
                            cx.notify();
                        }))
                })
        };
        let column = |id: &'static str,
                      scroll: &gpui::ScrollHandle,
                      editor: &Entity<mdoc_editor::EditorState>,
                      label: gpui::Stateful<gpui::Div>| {
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(
                    // Absolute, like the document scroll: intrinsic sizing must
                    // not measure (shape) the editor at extra widths.
                    div()
                        .id(id)
                        .absolute()
                        .inset_0()
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .px_6()
                        .pt(px(44.))
                        // Room for the view switcher over the last lines.
                        .pb(px(72.))
                        .on_scroll(cx.listener(|this, _, _, cx| {
                            this.wake_switcher(cx);
                            cx.notify();
                        }))
                        .child(editor.clone()),
                )
                .child(label)
        };
        let reply = state.mode == CompareMode::Reply;
        let (show_left, show_right) = match state.mode {
            CompareMode::Reply => (false, false),
            CompareMode::Original => (true, false),
            CompareMode::Pseudonymized => (false, true),
            CompareMode::Side => (
                !narrow || state.original_only,
                !narrow || !state.original_only,
            ),
        };
        let close = ui::floating_button(
            "close-compare",
            if reply {
                text::CLOSE_REPLY
            } else {
                text::CLOSE_SIDE_BY_SIDE
            },
            ui::Icon::Close,
            crate::ui::text::CLOSE,
            theme,
            true,
        )
        .when(cfg!(test), |v| v.debug_selector(|| "close-compare".into()))
        .on_click(cx.listener(|this, _, window, cx| this.close_compare(window, cx)));
        Some(
            div()
                .id("compare-view")
                .when(cfg!(test), |v| v.debug_selector(|| "compare-view".into()))
                .key_context("CompareView")
                .track_focus(&state.focus)
                .on_action(
                    cx.listener(|this, _: &crate::review::CloseCompare, window, cx| {
                        this.close_compare(window, cx)
                    }),
                )
                .relative()
                .size_full()
                .flex()
                // A column not shown stays laid out, invisible, under the shown
                // one: its scroll keeps following, so switching columns keeps
                // the reading position (Codex review).
                .child(
                    column(
                        "compare-original",
                        &state.left,
                        &state.original,
                        label("compare-label-original", text::ORIGINAL, &state.left),
                    )
                    .when(!show_left, |v| v.absolute().inset_0().invisible()),
                )
                .when(show_left && show_right, |v| {
                    v.child(div().w(px(1.)).h_full().bg(palette.border))
                })
                .child(
                    column(
                        "compare-current",
                        &state.right,
                        &state.current,
                        label("compare-label-current", text::PSEUDONYMIZED, &state.right),
                    )
                    .when(!show_right, |v| v.absolute().inset_0().invisible()),
                )
                // The restored reply over both, read-only.
                .when(reply, |v| {
                    v.child(
                        div()
                            .when(cfg!(test), |v| v.debug_selector(|| "reply-view".into()))
                            .absolute()
                            .inset_0()
                            .flex()
                            .bg(palette.bg)
                            .child(column(
                                "compare-reply",
                                &state.reply_scroll,
                                &state.reply,
                                label("compare-label-reply", text::REPLY, &state.reply_scroll),
                            )),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .top(px(FLOAT_TOP))
                        .right(px(20.))
                        .child(close),
                )
                .into_any_element(),
        )
    }
}
