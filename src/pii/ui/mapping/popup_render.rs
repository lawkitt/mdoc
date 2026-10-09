//! Word-anchored editor for the shared replacement workspace.
use super::*;
use gpui::{
    Animation, AnimationExt, AnyElement, SharedString, anchored, deferred, div, prelude::*,
    uniform_list,
};
use std::time::Duration;

/// The alias cue: one soft accent glow, `delay` ms after the popup opens.
fn cue_glow(delta: f32, delay: f32, accent: Hsla) -> Hsla {
    const TOTAL: f32 = 850.;
    let local = ((delta * TOTAL - delay) / 700.).clamp(0., 1.);
    Hsla {
        a: 0.24 * (local * std::f32::consts::PI).sin(),
        ..accent
    }
}
impl DocumentView {
    pub(super) fn readable_mention(
        &self,
        range: &Range<usize>,
        cx: &App,
    ) -> (String, String, String) {
        let source = self.editor.read(cx).text();
        if range.end > source.len()
            || !source.is_char_boundary(range.start)
            || !source.is_char_boundary(range.end)
        {
            return Default::default();
        }
        let start = source[..range.start]
            .char_indices()
            .rev()
            .nth(30)
            .map_or(0, |(i, _)| i);
        let end = source[range.end..]
            .char_indices()
            .nth(45)
            .map_or(source.len(), |(i, _)| range.end + i);
        fn clean(value: &str) -> String {
            static TAGS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            let value = TAGS
                .get_or_init(|| regex::Regex::new(r"<[^>]*>").unwrap())
                .replace_all(value, " ");
            value
                .replace(['*', '`', '|', '\\', '#'], "")
                .replace(['\n', '\r'], " ")
        }
        (
            clean(&source[start..range.start]),
            clean(&source[range.clone()]),
            clean(&source[range.end..end]),
        )
    }
    fn popup_control(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        enabled: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let id = id.into();
        let debug = id.to_string();
        self.pii.reveal_popup_control(
            id.clone(),
            crate::ui::control(id, label, self.theme.get(), enabled)
                .when(cfg!(test), |v| v.debug_selector(move || debug.clone())),
            cx,
        )
    }
    pub(in crate::pii::ui) fn replacement_popup(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let annotation = self.active_annotation()?;
        let id = self.selected_entity()?;
        let review = &self.pii.review;
        let identity = review.identity(id)?;
        let mapping = &self.pii.mapping;
        let original = self.active_original()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let enabled = !self.pii.scanning();
        // Cancel addition while this mention's addition is the latest step.
        let cancel = mapping
            .added_at(self.editor.read(cx).history_id())
            .is_some_and(|(added, _)| added.original == original);
        let applied = annotation & APPLIED_ID != 0;
        self.active_replacement_range()?;
        let mentions = self.identity_occurrences(id);
        let index = mentions
            .iter()
            .position(|(a, _)| *a == annotation)
            .unwrap_or(0);
        let wording = self.scoped_ranges(Scope::Wording).len();
        let selected_count = self.scoped_ranges(mapping.scope).len();
        let draft = mapping.alias.read(cx).value().trim();
        let changed = draft != identity.alias;
        let input_focused = mapping.alias.read(cx).focus_handle(cx).is_focused(window);
        let choices = mapping.pickers.alias || input_focused;
        let query = if changed {
            draft.to_lowercase()
        } else {
            String::new()
        };
        let targets: Arc<[_]> = if choices {
            self.alias_targets(&query, false).into()
        } else {
            Vec::new().into()
        };
        let available = self.scroll.bounds();
        let anchor = self.editor.read(cx).annotation_bounds(annotation);
        let hidden = self.editor.read(cx).annotation_is_hidden(annotation);
        let width = px(360.).min((available.size.width - px(16.)).max(px(160.)));
        let maximum = (available.size.height - px(44.)).max(px(110.));
        let desired = px(
            if choices || mapping.pickers.owner || mapping.pickers.category {
                430.
            } else {
                300.
            },
        )
        .min(maximum);
        let bounds = anchor.unwrap_or(gpui::Bounds::new(
            available.origin + gpui::point(px(8.), px(8.)),
            gpui::size(px(1.), px(18.)),
        ));
        let below = (available.bottom() - bounds.bottom() - px(8.)).max(px(0.));
        let above = (bounds.top() - available.top() - px(8.)).max(px(0.));
        let height = desired.min(below.max(above).max(px(100.)));
        let y = if below >= height || below >= above {
            bounds.bottom() + px(4.)
        } else {
            bounds.top() - height - px(4.)
        };
        let position = gpui::point(
            bounds.left().clamp(
                available.left() + px(4.),
                (available.right() - width - px(4.)).max(available.left() + px(4.)),
            ),
            y.max(available.top()),
        );
        let mut panel = crate::ui::panel("direct-replacement-popup", theme)
            .when(cfg!(test), |v| {
                v.debug_selector(|| "pseudonym-popup".into())
            })
            .key_context(if choices {
                "PseudonymReview UiPanel PiiChooser"
            } else {
                "PseudonymReview UiPanel"
            })
            .tab_group()
            .tab_stop(false)
            .track_focus(&self.pii.focus)
            .w(width)
            .max_h(height)
            .overflow_y_scroll()
            .track_scroll(&self.pii.popup_scroll)
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(13.))
            .on_action(cx.listener(|this, _: &crate::ui::NextControl, window, cx| {
                crate::ui::cycle(window, cx, Some(&this.pii.focus), false);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|this, _: &crate::ui::PreviousControl, window, cx| {
                    crate::ui::cycle(window, cx, Some(&this.pii.focus), true);
                    cx.stop_propagation();
                }),
            )
            .on_action(cx.listener(|this, _: &crate::PiiNextChoice, _, cx| {
                this.step_alias_choice(false, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::PiiPreviousChoice, _, cx| {
                this.step_alias_choice(true, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::PiiOpenChoice, window, cx| {
                this.open_alias_choice(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &PiiClosePopup, window, cx| {
                this.escape_popup(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &PiiConfirm, window, cx| {
                if this
                    .pii
                    .mapping
                    .alias
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
                {
                    this.open_alias_choice(window, cx);
                } else if this.pii.enter_applies
                    && this
                        .active_annotation()
                        .is_some_and(|a| a & APPLIED_ID == 0)
                {
                    // After a manual addition, Apply is the default action.
                    this.apply_scope(cx);
                }
                cx.stop_propagation();
            }))
            .on_mouse_down_out(
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    if this
                        .pii
                        .mapping
                        .bounds
                        .get()
                        .is_some_and(|b| b.contains(&event.position))
                    {
                        return;
                    }
                    this.close_pii_popup(&PiiClosePopup, window, cx);
                }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        self.popup_control(
                            "direct-category",
                            format!("{} ▾", identity.category.label()),
                            enabled,
                            cx,
                        )
                        .aria_label("Correct category for selected scope")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pii.mapping.toggle_category_picker();
                            cx.notify();
                        }))
                        .map(|v| match mapping.cue {
                            Some((generation, true)) => {
                                let accent = theme.search_accent();
                                v.with_animation(
                                    SharedString::from(format!("category-cue-{generation}")),
                                    Animation::new(Duration::from_millis(850))
                                        .with_easing(gpui::ease_in_out),
                                    move |v, delta| v.bg(cue_glow(delta, 150., accent)),
                                )
                                .into_any_element()
                            }
                            _ => v.into_any_element(),
                        }),
                    )
                    .when(applied, |v| {
                        let accent = theme.search_accent();
                        v.child(
                            div()
                                .flex_shrink_0()
                                .when(cfg!(test), |v| v.debug_selector(|| "direct-applied".into()))
                                .px_1()
                                .rounded_sm()
                                .text_size(px(11.))
                                .text_color(accent)
                                .bg(Hsla { a: 0.16, ..accent })
                                .child("Applied"),
                        )
                    })
                    .child(div().flex_1())
                    .when(cancel, |v| {
                        v.child(
                            self.pii
                                .reveal_popup_control(
                                    "direct-cancel-addition",
                                    crate::ui::icon_button(
                                        "direct-cancel-addition",
                                        "Cancel addition",
                                        crate::ui::Icon::Undo,
                                        theme,
                                        enabled,
                                    )
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "direct-cancel-addition".into())
                                    }),
                                    cx,
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.cancel_addition(window, cx)
                                })),
                        )
                    })
                    .child(
                        self.popup_control("direct-prev", "‹", mentions.len() > 1, cx)
                            .aria_label("Previous mention")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.step_identity_occurrence(true, window, cx)
                            })),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(palette.header_muted)
                            .child(format!("{} / {}", index + 1, mentions.len())),
                    )
                    .child(
                        self.popup_control("direct-next", "›", mentions.len() > 1, cx)
                            .aria_label("Next mention")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.step_identity_occurrence(false, window, cx)
                            })),
                    )
                    .child(
                        self.popup_control("direct-close", "×", true, cx)
                            .aria_label("Close replacement popup")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_pii_popup(&PiiClosePopup, window, cx)
                            })),
                    ),
            )
            .when(hidden, |v| {
                v.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.))
                        .text_color(palette.header_muted)
                        .child("Hidden source value · containing passage shown in document"),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "pseudonym-original".into())
                    })
                    .text_size(px(12.))
                    .child(original.to_string()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .id("direct-alias")
                    .when(cfg!(test), |v| v.debug_selector(|| "direct-alias".into()))
                    .w_full()
                    .relative()
                    .group("direct-alias")
                    .cursor(gpui::CursorStyle::IBeam)
                    .rounded_t_sm()
                    .border_b_1()
                    .border_color(theme.search_accent())
                    .text_color(theme.search_accent())
                    .child(mapping.alias.clone())
                    // A pencil on hover marks the alias as editable.
                    .child(
                        div()
                            .absolute()
                            .right(px(2.))
                            .top_0()
                            .bottom_0()
                            .flex()
                            .items_center()
                            .text_size(px(11.))
                            .text_color(palette.header_muted)
                            .opacity(0.)
                            .group_hover("direct-alias", |s| s.opacity(1.))
                            .child("✎"),
                    )
                    .map(|v| {
                        if mapping.target_index.is_none() {
                            crate::ui::reveal_focus(
                                v,
                                mapping.alias.read(cx).focus_handle(cx),
                                self.pii.popup_scroll.clone(),
                            )
                        } else {
                            v
                        }
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            // Select the token so typing replaces it.
                            this.pii.mapping.show_alias_choices();
                            this.pii
                                .mapping
                                .alias
                                .update(cx, |input, cx| input.select_all(cx));
                            cx.notify();
                        }),
                    )
                    .map(|v| match mapping.cue {
                        Some((generation, _)) => {
                            let accent = theme.search_accent();
                            v.with_animation(
                                SharedString::from(format!("alias-cue-{generation}")),
                                Animation::new(Duration::from_millis(850))
                                    .with_easing(gpui::ease_in_out),
                                move |v, delta| {
                                    let glow = cue_glow(delta, 0., accent);
                                    v.bg(glow).when(glow.a > 0.02, |v| v.border_b_2())
                                },
                            )
                            .into_any_element()
                        }
                        None => v.into_any_element(),
                    }),
            )
            .child(
                div().flex().flex_wrap().gap_1().children(
                    [Scope::Mention, Scope::Wording, Scope::Entity]
                        .into_iter()
                        .filter(|scope| *scope != Scope::Entity || mentions.len() > wording)
                        .map(|scope| {
                            let label = match scope {
                                Scope::Mention => "This mention".into(),
                                Scope::Wording => format!("Same wording · {wording}"),
                                Scope::Entity => format!("Entire entity · {}", mentions.len()),
                            };
                            self.popup_control(
                                SharedString::from(format!("scope-{}", scope as u8)),
                                label,
                                enabled,
                                cx,
                            )
                            .text_size(px(11.))
                            .when(mapping.scope == scope, |v| v.bg(palette.placeholder_bg))
                            // Hovering previews the chip's scope outline.
                            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                let mapping = &mut this.pii.mapping;
                                if *hovered {
                                    mapping.scope_hover = Some(scope);
                                } else if mapping.scope_hover == Some(scope) {
                                    mapping.scope_hover = None;
                                }
                                cx.notify();
                            }))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.pii.mapping.set_scope(scope);
                                    cx.notify();
                                },
                            ))
                        }),
                ),
            )
            .when_some(mapping.field_error.clone(), |v, error| {
                v.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .text_color(style::markdown_style(theme).alert_warning)
                        .child(error),
                )
            });
        if mapping.pickers.category {
            let custom = identity.custom_alias;
            panel = panel.child(div().flex().flex_wrap().gap_1().children(
                Category::ALL.into_iter().map(|category| {
                    let alias = if (custom || identity.category == category)
                        && selected_count == mentions.len()
                    {
                        identity.alias.clone()
                    } else {
                        review.next_alias(category)
                    };
                    self.popup_control(
                        SharedString::from(format!("direct-category-{}", category.token())),
                        format!("{} → {} · {selected_count}", category.label(), alias),
                        enabled,
                        cx,
                    )
                    .text_size(px(11.))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.correct_category(category, cx)),
                    )
                }),
            ));
        }
        if choices {
            let new_alias = review.next_alias(identity.category);
            panel = panel.child(
                self.popup_control(
                    "direct-new-alias",
                    format!("New alias · {new_alias}"),
                    enabled,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| this.separate_with_new_alias(cx))),
            );
            if changed {
                panel = panel
                    .child(
                        self.popup_control(
                            "direct-use-alias",
                            format!("Use {draft} · {selected_count} mentions"),
                            enabled,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_alias(false, Applying::Nothing, cx);
                            window.focus(&this.pii.focus, cx);
                        })),
                    )
                    .when(mentions.len() > selected_count, |v| {
                        v.child(
                            self.popup_control(
                                "direct-rename-all",
                                format!("Rename alias for all {} mentions", mentions.len()),
                                enabled,
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_alias(true, Applying::Nothing, cx);
                            })),
                        )
                    });
            }
            let count = targets.len();
            panel = panel.child(
                uniform_list(
                    "direct-alias-targets",
                    count,
                    cx.processor(move |this, indices: Range<usize>, _, cx| {
                        indices
                            .map(|index| {
                                let popup::AliasTarget {
                                    id: target,
                                    alias,
                                    original,
                                    category,
                                    ..
                                } = targets[index].clone();
                                this.popup_control(
                                    SharedString::from(format!("direct-target-{target}")),
                                    "",
                                    !this.pii.scanning(),
                                    cx,
                                )
                                .when(cfg!(test), |v| {
                                    v.debug_selector(move || format!("target-{target}"))
                                })
                                .aria_label(format!(
                                    "Use {alias}, {original}, {}",
                                    category.label()
                                ))
                                .w_full()
                                .when(this.pii.mapping.target_index == Some(index), |v| {
                                    v.bg(this.theme.get().pdf_style().placeholder_bg)
                                })
                                .h(px(48.))
                                .flex_col()
                                .items_start()
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .w_full()
                                        .text_ellipsis()
                                        .child(format!("{alias} · {}", category.label())),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .w_full()
                                        .text_ellipsis()
                                        .text_size(px(11.))
                                        .text_color(this.theme.get().pdf_style().header_muted)
                                        .child(original),
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.link_to_entity(target, cx);
                                        window.focus(&this.pii.focus, cx);
                                    },
                                ))
                            })
                            .collect()
                    }),
                )
                .h(px((count as f32 * 48.).clamp(0., 144.)))
                .flex_shrink_0()
                .track_scroll(&mapping.target_scroll),
            );
        }
        if !matches!(identity.category, Category::Person | Category::Organization) {
            let owner = identity
                .owner
                .and_then(|owner| review.identity(owner))
                .map(|owner| owner.alias.as_str())
                .unwrap_or("…");
            panel = panel.child(
                self.popup_control(
                    "direct-owner",
                    format!("Belongs to {owner} · all {} mentions", mentions.len()),
                    enabled,
                    cx,
                )
                .text_size(px(11.))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pii.mapping.toggle_owner_picker();
                    cx.notify();
                })),
            );
            if mapping.pickers.owner {
                panel = panel.child(mapping.target.clone()).child(
                    self.popup_control("direct-owner-none", "No owner", enabled, cx)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_mapping(MappingAction::Owner(id, None), cx)
                        })),
                );
                let query = mapping.target.read(cx).value().trim().to_lowercase();
                let owners: Arc<[_]> = self.alias_targets(&query, true).into();
                let count = owners.len();
                panel = panel.child(
                    uniform_list(
                        "direct-owners",
                        count,
                        cx.processor(move |this, indices: Range<usize>, _, cx| {
                            indices
                                .map(|index| {
                                    let popup::AliasTarget {
                                        id: target_id,
                                        alias,
                                        original,
                                        ..
                                    } = owners[index].clone();
                                    this.popup_control(
                                        SharedString::from(format!("direct-owner-{target_id}")),
                                        format!("{alias} · {original}"),
                                        !this.pii.scanning(),
                                        cx,
                                    )
                                    .w_full()
                                    .h(px(42.))
                                    .text_ellipsis()
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.change_mapping(
                                                MappingAction::Owner(id, Some(target_id)),
                                                cx,
                                            )
                                        },
                                    ))
                                })
                                .collect()
                        }),
                    )
                    .h(px((count as f32 * 42.).clamp(0., 126.)))
                    .flex_shrink_0(),
                );
            }
        }
        let (scoped_candidates, scoped_applied) = self.scoped_mentions();
        let in_scope = scoped_candidates.len() + scoped_applied.len();
        panel = panel.child(
            div()
                .flex_shrink_0()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .map(|v| {
                    if applied {
                        let ids: std::collections::HashSet<_> =
                            scoped_applied.iter().copied().collect();
                        let count = ids.len();
                        v.child(
                            self.pii
                                .reveal_popup_control(
                                    "direct-undo",
                                    crate::ui::icon_button(
                                        "direct-undo",
                                        format!("Undo replacement · {count} (back to proposed)"),
                                        crate::ui::Icon::Undo,
                                        theme,
                                        enabled && count > 0,
                                    )
                                    .when(cfg!(test), |v| {
                                        v.debug_selector(|| "direct-undo".into())
                                    }),
                                    cx,
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.undo_replacements(ids.clone(), cx)
                                })),
                        )
                    } else {
                        let count = scoped_candidates.len();
                        v.child(
                            self.popup_control(
                                "direct-apply",
                                format!("Apply · {count}"),
                                enabled && count > 0,
                                cx,
                            )
                            // The default (Enter) action, in the applied teal.
                            .when(self.pii.enter_applies && enabled && count > 0, |v| {
                                v.border_1()
                                    .border_color(theme.search_accent())
                                    .text_color(theme.search_accent())
                            })
                            .aria_label(format!("Apply {count} replacements in scope"))
                            .on_click(cx.listener(|this, _, _, cx| this.apply_scope(cx))),
                        )
                    }
                })
                .child(
                    self.popup_control(
                        "direct-keep",
                        format!("Keep original · {in_scope}"),
                        enabled && in_scope > 0,
                        cx,
                    )
                    .aria_label(format!("Keep {in_scope} originals in scope"))
                    .on_click(cx.listener(|this, _, window, cx| this.keep_originals(window, cx))),
                ),
        );

        // The editor records word geometry during paint, after this popup is rendered.
        // Reconcile once on the next frame so Apply and resize do not retain a stale anchor.
        let weak = cx.entity().downgrade();
        panel = panel.relative().child(
            gpui::canvas(
                move |_, window, _| {
                    let weak = weak.clone();
                    window.on_next_frame(move |_, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if this.active_annotation() == Some(annotation)
                                && this.editor.read(cx).annotation_bounds(annotation) != anchor
                            {
                                cx.notify();
                            }
                        });
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
        Some(
            deferred(anchored().position(position).snap_to_window().child(panel))
                .with_priority(2)
                .into_any_element(),
        )
    }
}
