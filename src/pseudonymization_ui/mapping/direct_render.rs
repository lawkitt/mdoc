//! Word-anchored editor for the shared replacement workspace.
use super::*;
use gpui::{AnyElement, SharedString, anchored, deferred, div, prelude::*, uniform_list};
impl Workspace {
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
    fn direct_control(
        &self,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        enabled: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let id = id.into();
        let debug = id.to_string();
        self.pseudonymization.reveal_popup_control(
            id.clone(),
            crate::ui::control(id, label, self.theme.get(), enabled)
                .when(cfg!(test), |v| v.debug_selector(move || debug.clone())),
            cx,
        )
    }
    pub(in crate::pseudonymization_ui) fn direct_replacement_popup(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let annotation = self.active_annotation()?;
        let id = self.selected_identity()?;
        let review = &self.pseudonymization.review;
        let identity = review.identity(id)?;
        let mapping = &self.pseudonymization.mapping;
        let original = self.active_original()?;
        let theme = self.theme.get();
        let palette = theme.pdf_style();
        let enabled = !self.pseudonymization.scanning();
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
        let choices = mapping.alias_choices || input_focused;
        let query = if changed {
            draft.to_lowercase()
        } else {
            String::new()
        };
        let targets: Arc<[_]> = if choices {
            self.direct_targets(&query, false).into()
        } else {
            Vec::new().into()
        };
        let available = self.scroll.bounds();
        let anchor = self.editor.read(cx).annotation_bounds(annotation);
        let hidden = self.editor.read(cx).annotation_is_hidden(annotation);
        let width = px(360.).min((available.size.width - px(16.)).max(px(160.)));
        let maximum = (available.size.height - px(44.)).max(px(110.));
        let desired = px(
            if choices || mapping.choosing_owner || mapping.choosing_category {
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
            .track_focus(&self.pseudonymization.focus)
            .w(width)
            .max_h(height)
            .overflow_y_scroll()
            .track_scroll(&self.pseudonymization.popup_scroll)
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(13.))
            .on_action(cx.listener(|this, _: &crate::ui::NextControl, window, cx| {
                crate::ui::cycle(window, cx, Some(&this.pseudonymization.focus), false);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|this, _: &crate::ui::PreviousControl, window, cx| {
                    crate::ui::cycle(window, cx, Some(&this.pseudonymization.focus), true);
                    cx.stop_propagation();
                }),
            )
            .on_action(cx.listener(|this, _: &crate::NextPiiChoice, _, cx| {
                this.step_alias_choice(false, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::PreviousPiiChoice, _, cx| {
                this.step_alias_choice(true, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::OpenPiiChoice, window, cx| {
                this.open_alias_choice(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ClosePseudonymPopup, window, cx| {
                this.close_direct_popup(window, cx);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|this, _: &AcceptPseudonymCandidate, window, cx| {
                    if this
                        .pseudonymization
                        .mapping
                        .alias
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                    {
                        this.open_alias_choice(window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down_out(
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    if this
                        .pseudonymization
                        .mapping
                        .bounds
                        .get()
                        .is_some_and(|b| b.contains(&event.position))
                    {
                        return;
                    }
                    this.close_pseudonym_popup(&ClosePseudonymPopup, window, cx);
                }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        self.direct_control(
                            "direct-category",
                            format!("{} ▾", identity.category.label()),
                            enabled,
                            cx,
                        )
                        .aria_label("Correct category for selected scope")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pseudonymization.mapping.choosing_category =
                                !this.pseudonymization.mapping.choosing_category;
                            cx.notify();
                        })),
                    )
                    .child(div().flex_1())
                    .child(
                        self.direct_control("direct-prev", "‹", mentions.len() > 1, cx)
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
                        self.direct_control("direct-next", "›", mentions.len() > 1, cx)
                            .aria_label("Next mention")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.step_identity_occurrence(false, window, cx)
                            })),
                    )
                    .child(
                        self.direct_control("direct-close", "×", true, cx)
                            .aria_label("Close replacement popup")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_pseudonym_popup(&ClosePseudonymPopup, window, cx)
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
                    .border_b_1()
                    .border_color(theme.search_accent())
                    .text_color(theme.search_accent())
                    .child(mapping.alias.clone())
                    .map(|v| {
                        if mapping.target_index.is_none() {
                            crate::ui::reveal_focus(
                                v,
                                mapping.alias.read(cx).focus_handle(cx),
                                self.pseudonymization.popup_scroll.clone(),
                            )
                        } else {
                            v
                        }
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.pseudonymization.mapping.alias_choices = true;
                            cx.notify();
                        }),
                    ),
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
                            self.direct_control(
                                SharedString::from(format!("scope-{}", scope as u8)),
                                label,
                                enabled,
                                cx,
                            )
                            .text_size(px(11.))
                            .when(mapping.scope == scope, |v| v.bg(palette.placeholder_bg))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.pseudonymization.mapping.scope = scope;
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
        if mapping.choosing_category {
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
                    self.direct_control(
                        SharedString::from(format!("direct-category-{}", category.token())),
                        format!("{} → {} · {selected_count}", category.label(), alias),
                        enabled,
                        cx,
                    )
                    .text_size(px(11.))
                    .on_click(cx.listener(move |this, _, _, cx| this.category_direct(category, cx)))
                }),
            ));
        }
        if choices {
            let new_alias = review.next_alias(identity.category);
            panel = panel.child(
                self.direct_control(
                    "direct-new-alias",
                    format!("New alias · {new_alias}"),
                    enabled,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| this.new_alias_direct(cx))),
            );
            if changed {
                panel = panel
                    .child(
                        self.direct_control(
                            "direct-use-alias",
                            format!("Use {draft} · {selected_count} mentions"),
                            enabled,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_alias_direct(false, false, cx);
                            window.focus(&this.pseudonymization.focus, cx);
                        })),
                    )
                    .when(mentions.len() > selected_count, |v| {
                        v.child(
                            self.direct_control(
                                "direct-rename-all",
                                format!("Rename alias for all {} mentions", mentions.len()),
                                enabled,
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_alias_direct(true, false, cx);
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
                                let direct::AliasTarget {
                                    id: target,
                                    alias,
                                    original,
                                    category,
                                    ..
                                } = targets[index].clone();
                                this.direct_control(
                                    SharedString::from(format!("direct-target-{target}")),
                                    "",
                                    !this.pseudonymization.scanning(),
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
                                .when(
                                    this.pseudonymization.mapping.target_index == Some(index),
                                    |v| v.bg(this.theme.get().pdf_style().placeholder_bg),
                                )
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
                                        this.link_direct(target, cx);
                                        window.focus(&this.pseudonymization.focus, cx);
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
                self.direct_control(
                    "direct-owner",
                    format!("Belongs to {owner} · all {} mentions", mentions.len()),
                    enabled,
                    cx,
                )
                .text_size(px(11.))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pseudonymization.mapping.choosing_owner =
                        !this.pseudonymization.mapping.choosing_owner;
                    cx.notify();
                })),
            );
            if mapping.choosing_owner {
                panel = panel.child(mapping.target.clone()).child(
                    self.direct_control("direct-owner-none", "No owner", enabled, cx)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.change_mapping(MappingAction::Owner(id, None), cx)
                        })),
                );
                let query = mapping.target.read(cx).value().trim().to_lowercase();
                let owners: Arc<[_]> = self.direct_targets(&query, true).into();
                let count = owners.len();
                panel = panel.child(
                    uniform_list(
                        "direct-owners",
                        count,
                        cx.processor(move |this, indices: Range<usize>, _, cx| {
                            indices
                                .map(|index| {
                                    let direct::AliasTarget {
                                        id: target_id,
                                        alias,
                                        original,
                                        ..
                                    } = owners[index].clone();
                                    this.direct_control(
                                        SharedString::from(format!("direct-owner-{target_id}")),
                                        format!("{alias} · {original}"),
                                        !this.pseudonymization.scanning(),
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
        let applied = annotation & APPLIED_ID != 0;
        let matching = if applied {
            let selected = review.tracking.get(annotation & !APPLIED_ID)?;
            review
                .tracking
                .applied
                .iter()
                .filter(|a| a.step.before == selected.step.before)
                .count()
        } else {
            let ranges: std::collections::HashSet<_> = self
                .scoped_ranges(Scope::Wording)
                .into_iter()
                .map(|r| (r.start, r.end))
                .collect();
            review
                .candidates
                .iter()
                .filter(|c| ranges.contains(&(c.range.start, c.range.end)))
                .count()
        };
        panel = panel.child(
            div()
                .flex_shrink_0()
                .flex()
                .flex_wrap()
                .gap_1()
                .child(
                    self.direct_control(
                        "direct-keep-restore",
                        if applied {
                            "Restore this mention"
                        } else {
                            "Keep this mention"
                        },
                        enabled,
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if applied {
                            this.restore_pii(&RestorePii, window, cx);
                        } else {
                            this.keep_replacement(true, cx);
                        }
                    })),
                )
                .when(matching > 1, |v| {
                    v.child(
                        self.direct_control(
                            "direct-keep-restore-all",
                            format!(
                                "{} {matching} matching values",
                                if applied { "Restore" } else { "Keep" }
                            ),
                            enabled,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                if applied {
                                    this.restore_all_pii(&RestoreAllPii, window, cx);
                                } else {
                                    this.keep_wording_direct(cx);
                                }
                            },
                        )),
                    )
                }),
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
