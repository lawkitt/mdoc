//! The document view through the window: its header and toolbar, preview,
//! find, zoom and spelling here, and the comment rail, decision history,
//! imports and notes in their own files.
pub(super) mod comment_rail;
pub(super) mod decision_history;
pub(super) mod import_flow;
pub(super) mod notes;
use super::*;

#[gpui::test]
fn copy_markdown_button_is_inert_while_markdown_is_empty(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let view = active(&tabs, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("kept".into()));
    more(cx, "more-copy-markdown");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("kept")
        );
        assert!(view.read(cx).copy_feedback.is_none());
    });
}

#[gpui::test]
fn copy_markdown_button_uses_active_tab_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let first = active(&tabs, cx);
    cx.simulate_input("first tab");
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    let second = active(&tabs, cx);
    cx.simulate_input("second tab");
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    more(cx, "more-copy-markdown");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("second tab")
        );
        assert!(first.read(cx).copy_feedback.is_none());
        assert!(second.read(cx).copy_feedback.is_some());
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.show_next_recent(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    more(cx, "more-copy-markdown");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("first tab")
        );
        assert_eq!(second.read(cx).editor.read(cx).text(), "second tab");
    });
}

#[gpui::test]
fn plus_keys_zoom_in_like_the_zoom_button(cx: &mut TestAppContext) {
    cx.update(crate::bind_app_keys);
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    let mut zoom = cx.update(|_, cx| view.read(cx).zoom);
    // Cmd/Ctrl with =, plus, and plus or = with the Shift a layout needs.
    for keys in [
        "secondary-=",
        "secondary-+",
        "secondary-shift-=",
        "secondary-shift-+",
    ] {
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
        let next = cx.update(|_, cx| view.read(cx).zoom);
        assert!(
            (next - zoom - crate::document::ZOOM_STEP).abs() < 1e-4,
            "{keys}: {zoom} -> {next}"
        );
        zoom = next;
    }
    cx.simulate_keystrokes("secondary-0");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).zoom, 1.));
}

#[gpui::test]
fn toolbar_preview_toggle_works_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_path(path.clone(), window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf().cloned().unwrap());
    click_toolbar(cx, "header-source");
    cx.update(|window, cx| {
        assert!(!view.read(cx).preview.visible());
        assert!(
            view.read(cx)
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
    click_toolbar(cx, "header-source");
    cx.update(|_, cx| {
        assert!(view.read(cx).preview.visible());
        assert_eq!(view.read(cx).preview.pdf(), Some(&pdf));
    });
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    click_toolbar(cx, "header-source");
    cx.update(|_, cx| {
        assert!(!view.read(cx).preview.visible());
        // Its Source is the viewing copy of the file.
        let attachment = view.read(cx).preview.attachment().cloned().unwrap();
        assert_eq!(attachment.file_name(), path.file_name());
    });
}

#[gpui::test]
fn detached_header_routes_preview_and_document_shortcuts(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    collapse(&tabs, cx);
    for theme in [Theme::Dark, Theme::Light] {
        tabs.update(cx, |tabs, cx| {
            tabs.theme.set(theme);
            cx.notify();
        });
        click_toolbar(cx, "header-source");
        cx.update(|_, cx| assert!(!view.read(cx).preview.visible()));
        click_toolbar(cx, "header-source");
        cx.update(|_, cx| assert!(view.read(cx).preview.visible()));
        click_toolbar(cx, "header-more");
        let zoom = cx.update(|_, cx| view.read(cx).zoom);
        cx.dispatch_action(ZoomIn);
        cx.run_until_parked();
        cx.update(|_, cx| assert!(view.read(cx).zoom > zoom));
        cx.dispatch_action(ZoomReset);
        click_toolbar(cx, "more-copy-markdown");
    }
}

#[gpui::test]
fn preview_find_types_through_text_input(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf().cloned().unwrap());
    // The find button's action; Mod+F opens the Markdown find.
    pdf.update_in(cx, |pdf, window, cx| pdf.toggle_search(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| pdf.read(cx).is_search_open()));
    // Composed and non-ASCII text arrives through the input handler, not key
    // codes, so IME and other layouts reach the query.
    cx.simulate_input("Договор 契約");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| pdf.read(cx).search_query().to_owned()),
        "Договор 契約"
    );
    cx.simulate_keystrokes("backspace backspace");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| pdf.read(cx).search_query().to_owned()),
        "Договор "
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| pdf.read(cx).is_search_open()));
    assert!(cx.update(|window, cx| pdf.read(cx).focus_handle(cx).is_focused(window)));
}

#[gpui::test]
fn preview_find_composes_japanese(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
    let mut field = None;
    let (pdf, cx) = cx.add_window_view(|_, cx| {
        let mut pdf = PdfView::new(path, Rc::new(Default::default), Rc::new(|| 1.), cx);
        field = Some(crate::document::preview::attach_search_field(&mut pdf, cx));
        pdf
    });
    let field = field.unwrap();
    pdf.update_in(cx, |pdf, window, cx| pdf.toggle_search(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|window, cx| field.read(cx).focus_handle(cx).is_focused(window)));
    // What the IME sends: marked kana while composing, then the chosen kanji.
    field.update_in(cx, |field, window, cx| {
        field.replace_and_mark_text_in_range(None, "けいやく", None, window, cx);
        assert_eq!(field.marked_text_range(window, cx), Some(0..4));
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| pdf.read(cx).search_query().to_owned()),
        "けいやく"
    );
    field.update_in(cx, |field, window, cx| {
        field.replace_text_in_range(None, "契約", window, cx);
        assert_eq!(field.marked_text_range(window, cx), None);
    });
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| pdf.read(cx).search_query().to_owned()),
        "契約"
    );
}

#[gpui::test]
fn paired_preview_hides_reopens_and_failed_replacement_preserves_attachment(
    cx: &mut TestAppContext,
) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pdf = base.join("../../tests/fixtures/reference.pdf");
    view.update_in(cx, |view, window, cx| {
        view.open_path(pdf.clone(), window, cx)
    });
    cx.run_until_parked();
    let entity = cx.update(|_, cx| view.read(cx).preview.pdf().cloned().unwrap());
    cx.dispatch_action(TogglePreview);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).preview.visible());
        assert_eq!(view.read(cx).preview.attachment(), Some(&pdf));
    });
    cx.dispatch_action(TogglePreview);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).preview.pdf(), Some(&entity)));
    view.update_in(cx, |view, window, cx| {
        view.open_path(base.join("missing.pdf"), window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).preview.pdf(), Some(&entity));
        assert_eq!(view.read(cx).preview.attachment(), Some(&pdf));
        assert!(view.read(cx).preview.retryable());
    });
}

#[gpui::test]
fn document_header_keeps_its_actions_in_a_narrow_window_in_both_themes(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.editor
            .update(cx, |e, cx| e.set_text("Anna Petrova signed.", cx))
    });
    cx.simulate_resize(size(px(720.), px(480.)));
    for theme in [Theme::Dark, Theme::Light] {
        for expanded in [false, true] {
            for source_only in [false, true] {
                tabs.update(cx, |tabs, cx| {
                    tabs.theme.set(theme);
                    tabs.sidebar.choose(Some(expanded));
                    cx.notify();
                });
                view.update(cx, |view, cx| {
                    view.lifecycle.set_source_only(source_only);
                    cx.notify();
                });
                for _ in 0..3 {
                    draw(cx);
                    cx.run_until_parked();
                }
                // One header row: ⋯ and the primary action inside it.
                // Collapsed, it is the title row, level with the traffic lights.
                let header = cx.debug_bounds("document-toolbar").unwrap();
                let row = if expanded {
                    style::HEADER_HEIGHT
                } else {
                    style::CARD_MARGIN + style::HEADER_HEIGHT
                };
                assert_eq!(header.size.height, px(row));
                let primary = if source_only {
                    "primary-convert"
                } else {
                    "Pseudonymize"
                };
                for selector in ["header-more", primary] {
                    let bounds = cx.debug_bounds(selector).unwrap();
                    assert!(
                        header.contains(&bounds.origin) && header.contains(&bounds.bottom_right()),
                        "{selector} {bounds:?} outside {header:?}"
                    );
                    assert!(bounds.right() <= px(720.));
                }
                // Settings and the theme control sit at the sidebar's foot.
                assert_eq!(cx.debug_bounds("theme-toggle").is_some(), expanded);
                assert!(cx.debug_bounds("Copy Markdown").is_none());
            }
        }
    }
}

#[gpui::test]
fn markdown_scrollbar_drags_without_changing_preview_scroll(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1200.), px(800.)));
    let view = active(&tabs, cx);
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
    view.update_in(cx, |view, window, cx| {
        view.editor.update(cx, |editor, cx| {
            editor.set_text("# Heading\n\nA paragraph with words.\n\n".repeat(200), cx)
        });
        view.open_path(pdf, window, cx);
    });
    cx.run_until_parked();
    let draw = |cx: &mut VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            cx.run_until_parked();
        }
    };
    draw(cx);
    let preview = cx.update(|_, cx| view.read(cx).preview.pdf().cloned().unwrap());
    let before = cx.update(|_, cx| preview.read(cx).reading_position());
    let thumb = cx
        .debug_bounds("markdown-scrollbar")
        .expect("overflowing Markdown scrollbar");
    cx.simulate_mouse_down(thumb.center(), gpui::MouseButton::Left, Default::default());
    for y in [40., 100.] {
        cx.simulate_mouse_move(
            thumb.center() + gpui::point(px(0.), px(y)),
            Some(gpui::MouseButton::Left),
            Default::default(),
        );
    }
    cx.simulate_mouse_up(
        thumb.center() + gpui::point(px(0.), px(100.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    draw(cx);
    cx.update(|_, cx| {
        assert!(view.read(cx).scroll.offset().y < px(-100.));
        assert_eq!(view.read(cx).scroll.offset().x, px(0.));
        assert_eq!(preview.read(cx).reading_position(), before);
    });
}

#[gpui::test]
fn narrow_original_switch_and_divider_preserve_source_and_session(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    view.update(cx, |v, cx| {
        v.editor
            .update(cx, |e, cx| e.set_text("# Draft\n\nOriginal words", cx));
        v.lifecycle.set_source_only(false);
        cx.notify();
    });
    let (source, revision) = cx.update(|_, cx| {
        (
            view.read(cx).editor.read(cx).text().to_owned(),
            view.read(cx).editor.read(cx).revision(),
        )
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let divider = cx.debug_bounds("preview-divider").unwrap();
    cx.simulate_mouse_down(
        divider.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        divider.center() + gpui::point(px(80.), px(0.)),
        Some(gpui::MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_up(
        divider.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    let ratio = cx.update(|_, cx| view.read(cx).preview.split_ratio().unwrap());
    assert!(ratio > 0.5);
    cx.simulate_resize(size(px(640.), px(480.)));
    expand_sidebar(&tabs, cx);
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    for theme in [Theme::Dark, Theme::Light] {
        view.update(cx, |v, cx| {
            v.theme.set(theme);
            v.editor.update(cx, |e, cx| {
                e.set_markdown_style(style::markdown_style(theme), cx)
            });
            v.sync_pii_theme(cx);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        for selector in [
            "header-more",
            "header-source",
            "Pseudonymize",
            "Settings",
            "theme-toggle",
            "Markdown",
            "Source",
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(bounds.left() >= px(0.) && bounds.right() <= px(640.));
            assert!(bounds.bottom() <= px(480.));
        }
    }
    assert!(cx.debug_bounds("preview-divider").is_none());
    click_toolbar(cx, "Source");
    cx.update(|_, cx| assert!(view.read(cx).original_selected));
    click_toolbar(cx, "Markdown");
    cx.simulate_resize(size(px(1100.), px(760.)));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("preview-divider").is_some());
    cx.update(|_, cx| {
        let v = view.read(cx);
        assert_eq!(v.editor.read(cx).text(), source);
        assert_eq!(v.editor.read(cx).revision(), revision);
        assert_eq!(v.preview.split_ratio(), Some(ratio));
        assert!(v.preview.visible());
    });
    let snapshot = tabs.update(cx, |tabs, cx| tabs.snapshot(cx));
    let decoded: Session = serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(
        decoded
            .tabs
            .iter()
            .find(|t| t.preview_split.is_some())
            .unwrap()
            .preview_split,
        Some(ratio)
    );
}

#[gpui::test]
fn spelling_menu_item_switches_the_document(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let view = active(&tabs, cx);
    view.update(cx, |view, cx| {
        view.editor.update(cx, |editor, cx| {
            editor.set_text("Frist and secnd words.", cx)
        });
    });
    cx.run_until_parked();
    draw(cx);
    cx.update(|_, cx| assert!(!view.read(cx).editor.read(cx).diagnostics().is_empty()));

    // Spelling lives in the header's ⋯ menu.
    more(cx, "more-spelling");
    cx.update(|_, cx| {
        assert!(view.read(cx).spelling.off_here());
        assert!(!view.read(cx).more_open);
        assert!(view.read(cx).editor.read(cx).diagnostics().is_empty());
    });
    more(cx, "more-spelling");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!view.read(cx).spelling.off_here());
        assert!(!view.read(cx).editor.read(cx).diagnostics().is_empty());
    });
}
