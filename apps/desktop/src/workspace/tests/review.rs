//! The pseudonymization review through the window: Copy for AI's reasons
//! and the identity list here, and edited text, hand copies and restore in
//! place in their own files.
pub(super) mod edited_text;
pub(super) mod hand_copies;
pub(super) mod restore_in_place;
use super::*;

#[gpui::test]
fn disabled_copy_for_ai_says_why(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let view = active(&tabs, cx);
    // Empty, nothing to find: off, with its reason under the label, so the
    // row is taller than Copy Markdown's.
    click_toolbar(cx, "header-more");
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.alias_actions_reason().is_some());
        assert_eq!(view.copy_step(cx), None);
    });
    let disabled = cx.debug_bounds("more-copy-for-ai").unwrap();
    let enabled = cx.debug_bounds("more-copy-markdown").unwrap();
    assert!(disabled.size.height > enabled.size.height);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("kept".into()));
    click_toolbar(cx, "more-copy-for-ai");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("kept")
        );
    });
}

/// #1209: a document never scanned is refused with its step, not a dead
/// row: ⋯ › Copy for AI copies nothing and the notice offers "Find names".
#[gpui::test]
fn copy_for_ai_before_a_scan_offers_find_names(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let view = active(&tabs, cx);
    cx.simulate_input("Maya Bennett signed.");
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).copy_step(cx),
            Some(crate::review::refusal::Step::FindNames)
        );
    });
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("kept".into()));
    more(cx, "more-copy-for-ai");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("kept")
        );
        let refusal = view.read(cx).copy_refusal.clone().expect("refused");
        assert_eq!(refusal.step, Some(crate::review::refusal::Step::FindNames));
    });
}

/// Public synthetic input through the real editor/review/Project host. Timings are
/// observations until release/native budgets are qualified.
#[gpui::test]
#[ignore = "review/diagnostics/compare/Project CPU matrix; run serially on an idle machine"]
fn review_host_performance(cx: &mut TestAppContext) {
    use crate::perf_tests::{draw, report};
    use mdoc_pii::Category;
    use std::time::Instant;
    let stress = std::env::var("MDOC_PERF_STRESS").is_ok_and(|v| v == "1");
    let (entities, mentions) = if stress { (64, 20_000) } else { (16, 160) };
    let names: Vec<_> = (0..entities).map(|i| format!("ClientName{i:04}")).collect();
    let wordings: Vec<_> = names
        .iter()
        .map(|n| (n.as_str(), Category::Person))
        .collect();
    let mut text: String = (0..mentions)
        .map(|i| {
            format!(
                "{} agrees to the agreemnt. Проверить условия.\n\n",
                names[i % entities]
            )
        })
        .collect();
    if stress {
        text.push_str(&" ".repeat(2_097_152_usize.saturating_sub(text.len())));
    }
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    // Layout-only baseline: the same text without review state, with the
    // Replacements panel open so the editor width matches the review phases.
    let plain = dir.path().join("plain.md");
    std::fs::write(&plain, &text).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![plain], window, cx)
    });
    cx.run_until_parked();
    let plain_view = active(&tabs, cx);
    plain_view.update_in(cx, |view, window, cx| {
        view.inspector = crate::document::Inspector::Review;
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
    });
    cx.run_until_parked();
    draw(cx);
    draw(cx);
    let mut samples = Vec::new();
    for _ in 0..7 {
        let start = Instant::now();
        draw(cx);
        samples.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let plain_width =
        plain_view.read_with(cx, |v, cx| v.editor.read(cx).layout_width().map(f32::from));
    eprintln!(
        "REVIEW_PERF phase=plain_layout bytes={} editor_width={plain_width:?}",
        text.len()
    );
    report("review_plain_layout", &mut samples);
    plain_view.read_with(cx, |view, _| {
        assert!(
            view.pii.review().candidates().is_empty() && view.pii.review().applied().is_empty()
        );
    });
    // Each file its own bytes: the same bytes again would ask Open Existing.
    let first = open_applied(
        &tabs,
        cx,
        dir.path(),
        "first.md",
        &format!("{text}1"),
        &wordings,
    );
    let first_view = active(&tabs, cx);
    let project_id = new_project(&tabs, cx, first);
    for name in ["second.md", "third.md"] {
        let member = open_applied(
            &tabs,
            cx,
            dir.path(),
            name,
            &format!("{text}{name}"),
            &wordings,
        );
        move_tab(&tabs, cx, member, Some(&project_id));
    }
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(first, window, cx));
    for phase in ["proposed_panel", "applied_panel", "before_after"] {
        first_view.update_in(cx, |view, window, cx| {
            match phase {
                "proposed_panel" => view
                    .editor
                    .update(cx, |editor, cx| editor.undo_step(window, cx)),
                "applied_panel" => view.apply_all_pii(&PiiApplyAll, window, cx),
                _ => view.toggle_compare(window, cx),
            }
            view.inspector = crate::document::Inspector::Review;
            if !view.replacements_open() {
                view.toggle_replacements_panel(window, cx);
            }
        });
        cx.run_until_parked();
        draw(cx);
        draw(cx);
        let mut samples = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            draw(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
        eprintln!(
            "REVIEW_PERF phase={phase} bytes={} entities={entities} mentions={mentions} editor_width={:?}",
            text.len(),
            first_view.read_with(cx, |v, cx| v.editor.read(cx).layout_width().map(f32::from))
        );
        report(&format!("review_{phase}"), &mut samples);
        assert_eq!(
            first_view.read_with(cx, |v, cx| v.editor.read(cx).layout_width().map(f32::from)),
            plain_width,
            "review phases and the layout baseline share one editor width"
        );
        first_view.read_with(cx, |view, _| {
            assert_eq!(
                view.pii.review().candidates().len() + view.pii.review().applied().len(),
                mentions
            );
            if phase == "proposed_panel" {
                assert_eq!(view.pii.review().candidates().len(), mentions);
            } else {
                assert_eq!(view.pii.review().applied().len(), mentions);
            }
            assert_eq!(view.compare.is_some(), phase == "before_after");
            assert!(
                view.inspector != crate::document::Inspector::Share,
                "measure the Review tab, not the post-Apply Share tab"
            );
        });
    }
    let start = Instant::now();
    first_view.update_in(cx, |view, window, cx| {
        view.close_compare(window, cx);
        let id = view.pii.review().identity_by_alias("PERSON_1").unwrap();
        view.retag_identities(&[(id, "CLIENT".into(), Category::Person, true)], cx)
            .unwrap();
    });
    tabs.update(cx, |tabs, cx| tabs.project_changed(first, cx));
    cx.run_until_parked();
    draw(cx);
    eprintln!(
        "REVIEW_PERF project_fanout_3_members_ms={:.3} rss_kib={:?}",
        start.elapsed().as_secs_f64() * 1000.,
        crate::perf_tests::rss_kib()
    );
    tabs.read_with(cx, |tabs, cx| {
        let members: Vec<_> = tabs
            .tabs
            .iter()
            .filter_map(|tab| tab.view())
            .filter(|view| {
                view.read(cx)
                    .project
                    .link()
                    .is_some_and(|link| link.id == project_id)
            })
            .collect();
        assert_eq!(members.len(), 3);
        for view in members {
            assert!(
                view.read(cx)
                    .pii
                    .review()
                    .identity_by_alias("CLIENT")
                    .is_some()
            );
        }
    });
}

/// The Replacements panel's header toggle expands or collapses every
/// identity, by pointer or keyboard, and its name follows the state; the
/// palette has both commands while the panel is open.
#[gpui::test]
fn expand_and_collapse_all_identities(cx: &mut TestAppContext) {
    cx.update(crate::workspace::bind_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
        // Apply all leaves the inspector on Share; the list is on Review.
        view.inspector = crate::document::Inspector::Review;
        cx.notify();
    });
    cx.run_until_parked();
    draw(cx);
    let state = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let view = view.read(cx);
            (view.expanded_identities(), view.all_identities_expanded(cx))
        })
    };
    assert_eq!(state(cx), (0, false));
    // Pointer: expand all, then collapse all.
    click_toolbar(cx, "replacement-expand-all");
    assert_eq!(state(cx), (2, true));
    click_toolbar(cx, "replacement-expand-all");
    assert_eq!(state(cx), (0, false));
    // Keyboard: Tab through the panel to the toggle, then Enter.
    view.update_in(cx, |view, window, cx| {
        window.focus(&view.replacements_focus(), cx)
    });
    draw(cx);
    let toggle = cx.update(|_, cx| view.read(cx).expand_all_focus());
    let mut on_toggle = false;
    for _ in 0..30 {
        key(cx, "tab");
        on_toggle = cx.update(|window, _| toggle.is_focused(window));
        if on_toggle {
            break;
        }
    }
    assert!(on_toggle, "Tab reaches the toggle");
    activate_control(cx);
    assert_eq!(state(cx), (2, true));
    // Palette: both commands, run with focus in the sidebar, outside the
    // document's own action handlers.
    for (query, expanded) in [("collapse all identities", 0), ("expand all identities", 2)] {
        tabs.update_in(cx, |tabs, window, cx| {
            let sidebar = tabs.focus_regions.sidebar().clone();
            window.focus(&sidebar, cx);
        });
        draw(cx);
        cx.simulate_keystrokes("secondary-k");
        draw(cx);
        cx.simulate_input(query);
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(state(cx).0, expanded, "{query}");
    }
}
