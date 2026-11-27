//! The comment rail in the real tab shell: the keyboard flow, the "comments"
//! F6 region, the app-wide Source toggles and markers in a narrow window.
use crate::document::comment_rail::{CommentRail, NextComment, PreviousComment};
use crate::workspace::tests::*;

/// comments.docx open in a window wide enough for the rail; its threads in
/// rail order are "0" (with a reply), "1", the point "2", the resolved "4",
/// and "3", which has no place.
fn open_comments(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(width), px(height)));
    let docx = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/docx-preview/comments.docx");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(docx, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.preview.set_split(0.25);
        cx.notify();
    });
    for _ in 0..3 {
        cx.run_until_parked();
        draw(cx);
    }
    (tabs, cx)
}

fn rail(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<CommentRail> {
    let view = active(tabs, cx);
    cx.update(|_, cx| view.read(cx).preview.comment_rail().cloned().unwrap())
}

fn pdf(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<PdfView> {
    let view = active(tabs, cx);
    cx.update(|_, cx| view.read(cx).preview.pdf().cloned().unwrap())
}

fn focused_card(rail: &Entity<CommentRail>, cx: &mut VisualTestContext) -> Option<usize> {
    cx.update(|window, cx| {
        let rail = rail.read(cx);
        (0..rail.count()).find(|&i| rail.card_focus(i).is_focused(window))
    })
}

/// Press the Source header's toggle `n`, as its click does.
fn toggle(pdf: &Entity<PdfView>, n: usize, cx: &mut VisualTestContext) {
    let action = cx.update(|_, cx| pdf.read(cx).header_toggles()[n].action.clone());
    cx.update(|window, cx| action(window, cx));
    cx.run_until_parked();
    draw(cx);
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn the_rail_is_a_region_walked_by_keyboard_in_page_order(cx: &mut TestAppContext) {
    let (tabs, cx) = open_comments(cx, 1600., 900.);
    let rail = rail(&tabs, cx);
    let pdf = pdf(&tabs, cx);
    cx.update(|_, cx| {
        assert!(rail.read(cx).shown());
        assert_eq!(rail.read(cx).count(), 5);
        assert!(
            pdf.read(cx).layout().rail_width > 0.,
            "a wide pane has the rail"
        );
        let view = tabs.read(cx).active_view().unwrap();
        let region = view.read(cx).comments_region.clone();
        assert!(
            view.read(cx)
                .focus_regions(cx)
                .iter()
                .any(|(r, _)| *r == region)
        );
    });
    // From the page, Next comment focuses the first card on it, then the next.
    cx.update(|window, cx| window.focus(&pdf.read(cx).focus_handle(cx), cx));
    cx.dispatch_action(NextComment);
    draw(cx);
    assert_eq!(focused_card(&rail, cx), Some(0));
    cx.dispatch_action(NextComment);
    draw(cx);
    assert_eq!(focused_card(&rail, cx), Some(1));
    cx.dispatch_action(PreviousComment);
    draw(cx);
    assert_eq!(focused_card(&rail, cx), Some(0));
    // Enter expands the focused card, again collapses it.
    activate_control(cx);
    cx.update(|_, cx| assert_eq!(rail.read(cx).open(), Some(0)));
    activate_control(cx);
    cx.update(|_, cx| assert_eq!(rail.read(cx).open(), None));
    // Tab walks on to the next card.
    key(cx, "tab");
    assert_eq!(focused_card(&rail, cx), Some(1));
    // Esc returns to the page.
    key(cx, "escape");
    cx.update(|window, cx| assert!(pdf.read(cx).focus_handle(cx).is_focused(window)));
    // From the last thread on the page, Next comment wraps to the start of
    // the rail: "Not on the page", which opens.
    cx.update(|window, cx| window.focus(&rail.read(cx).card_focus(3), cx));
    cx.dispatch_action(NextComment);
    draw(cx);
    assert_eq!(focused_card(&rail, cx), Some(4));
    cx.update(|_, cx| assert!(rail.read(cx).nowhere_open()));
}

#[gpui::test]
fn a_click_on_a_passage_expands_its_card_and_strengthens_its_tint(cx: &mut TestAppContext) {
    let (tabs, cx) = open_comments(cx, 1600., 900.);
    let rail = rail(&tabs, cx);
    let pdf = pdf(&tabs, cx);
    let marks = |cx: &mut VisualTestContext| cx.update(|_, cx| rail.read(cx).marks_for_test());
    let before = marks(cx);
    // The resolved thread has no tint; the others do, none underlined.
    assert!(before.iter().all(|m| m.key != 3 && m.underline.is_none()));
    assert!(before.iter().any(|m| m.key == 1));
    pdf.update(cx, |_, cx| cx.emit(gpui_pdf::PdfEvent::MarkClicked(1)));
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(rail.read(cx).open(), Some(1)));
    let after = marks(cx);
    let active = after.iter().find(|m| m.key == 1).unwrap();
    let was = before.iter().find(|m| m.key == 1).unwrap();
    assert!(active.underline.is_some(), "never colour alone");
    assert!(active.fill.a > was.fill.a);
    // The resolved chip expands to a card, and its passage then shows.
    click(cx, "comment-card-3");
    cx.update(|_, cx| assert_eq!(rail.read(cx).open(), Some(3)));
    assert!(marks(cx).iter().any(|m| m.key == 3));
}

#[gpui::test]
fn the_source_toggles_are_app_wide_settings(cx: &mut TestAppContext) {
    let (tabs, cx) = open_comments(cx, 1600., 900.);
    let rail = rail(&tabs, cx);
    let pdf = pdf(&tabs, cx);
    let general = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            tabs.read(cx)
                .preferences
                .borrow()
                .current
                .as_ref()
                .unwrap()
                .general
        })
    };
    let toggles = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            pdf.read(cx)
                .header_toggles()
                .iter()
                .map(|t| (t.label.to_string(), t.on))
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        toggles(cx),
        // Five threads, one with a reply: every comment counts (#964).
        [("💬 6".into(), true), ("Hidden text (1)".into(), false)]
    );
    // Hidden: the plain page, no tint, no region.
    toggle(&pdf, 0, cx);
    assert!(!general(&tabs, cx).show_comments);
    cx.update(|_, cx| {
        assert!(!rail.read(cx).shown());
        assert!(rail.read(cx).marks_for_test().is_empty());
        assert_eq!(pdf.read(cx).layout().rail_width, 0.);
        let view = tabs.read(cx).active_view().unwrap();
        let region = view.read(cx).comments_region.clone();
        assert!(
            !view
                .read(cx)
                .focus_regions(cx)
                .iter()
                .any(|(r, _)| *r == region)
        );
    });
    assert_eq!(toggles(cx)[0], ("💬 6".into(), false));
    toggle(&pdf, 0, cx);
    assert!(general(&tabs, cx).show_comments);
    cx.update(|_, cx| assert!(rail.read(cx).shown()));
    // Hidden text draws the page again, where it was, with the text shown.
    toggle(&pdf, 1, cx);
    for _ in 0..3 {
        cx.run_until_parked();
        draw(cx);
    }
    assert!(general(&tabs, cx).show_hidden_text);
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        let docx = view.read(cx).preview.docx().unwrap();
        assert!(docx.options.show_hidden_text);
        assert_eq!(docx.hidden_text, 1);
        let pdf = view.read(cx).preview.pdf().unwrap().read(cx);
        assert_eq!(pdf.header_toggles()[1].label.to_string(), "Hidden text (1)");
        assert!(pdf.header_toggles()[1].on);
    });
}

#[gpui::test]
fn a_narrow_window_shows_markers_that_open_cards_under_their_line(cx: &mut TestAppContext) {
    let (tabs, cx) = open_comments(cx, 640., 480.);
    let view = active(&tabs, cx);
    // One pane at a time: the Source.
    view.update(cx, |view, cx| {
        view.original_selected = true;
        cx.notify();
    });
    cx.run_until_parked();
    draw(cx);
    let rail = rail(&tabs, cx);
    let pdf = pdf(&tabs, cx);
    cx.update(|_, cx| assert_eq!(pdf.read(cx).layout().rail_width, 0.));
    assert!(
        cx.debug_bounds("comment-card-0").is_none(),
        "cards wait closed"
    );
    click(cx, "comment-marker-0");
    cx.update(|_, cx| assert_eq!(rail.read(cx).open(), Some(0)));
    assert!(cx.debug_bounds("comment-card-0").is_some());
    // Esc from the card closes it and returns to the page.
    cx.update(|window, cx| window.focus(&rail.read(cx).card_focus(0), cx));
    key(cx, "escape");
    cx.update(|window, cx| {
        assert_eq!(rail.read(cx).open(), None);
        assert!(pdf.read(cx).focus_handle(cx).is_focused(window));
    });
    // Esc from a marker returns to the page too.
    cx.update(|window, cx| window.focus(&rail.read(cx).marker_focus(1), cx));
    key(cx, "escape");
    cx.update(|window, cx| assert!(pdf.read(cx).focus_handle(cx).is_focused(window)));
    // Next comment opens the next line's card and focuses it.
    cx.dispatch_action(NextComment);
    draw(cx);
    assert_eq!(focused_card(&rail, cx), Some(0));
    cx.update(|_, cx| assert_eq!(rail.read(cx).open(), Some(0)));
}

/// A PDF's annotations take the same rail, read for its Source whether or
/// not its import ran here, and the left-out line stops counting them.
#[gpui::test]
fn a_pdf_shows_its_annotations_on_the_rail(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1600.), px(900.)));
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/pdf-source/comments.pdf");
    let view = tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
        let view = tabs.active_view().unwrap();
        view.update(cx, |view, cx| {
            view.preview.set_split(0.25);
            view.session.comments_left_out = Some("10 comments not shown.".into());
            view.open_pdf(source, window, cx);
        });
        view
    });
    for _ in 0..3 {
        cx.run_until_parked();
        draw(cx);
    }
    let rail = rail(&tabs, cx);
    let pdf = pdf(&tabs, cx);
    cx.update(|_, cx| {
        // Ten annotations; two are replies.
        assert_eq!(rail.read(cx).count(), 8);
        assert!(rail.read(cx).shown());
        assert_eq!(pdf.read(cx).header_toggles().len(), 1, "no hidden text");
        // The label counts the replies too (#964).
        assert_eq!(pdf.read(cx).header_toggles()[0].label.as_ref(), "💬 10");
        assert_eq!(view.read(cx).left_out(), None);
    });
    // The highlight's tint lies on page 1's second line.
    let marks = cx.update(|_, cx| rail.read(cx).marks_for_test());
    let cap = marks.iter().find(|m| m.key == 1).unwrap();
    assert_eq!(cap.page, 0);
    assert!((cap.rects[0].y - 110. / 720.).abs() < 0.002);
    // Comments off: no rail, no tints.
    toggle(&pdf, 0, cx);
    cx.update(|_, cx| {
        assert!(!rail.read(cx).shown());
        assert!(rail.read(cx).marks_for_test().is_empty());
    });
    toggle(&pdf, 0, cx);
}
