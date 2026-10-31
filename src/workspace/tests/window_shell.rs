//! The window shell: toolbar, menus, palette, Settings, Home and AI replies.
use super::*;

#[gpui::test]
fn disabled_copy_for_ai_says_why(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let view = active(&tabs, cx);
    cx.simulate_input("Maya Bennett signed.");
    click_toolbar(cx, "header-more");
    draw(cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).alias_actions_reason(),
            Some("Pseudonymize the document first.")
        );
    });
    // The reason sits under the label, so the row is taller than Copy
    // Markdown's.
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
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle(-1, window, cx));
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
fn toolbar_settings_opens_and_closes_without_editor_focus(cx: &mut TestAppContext) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    // Keep the direct-click targets visible with the model attribution rows.
    // Scrolling keyboard reach is covered separately; below we resize small
    // to verify that the dialog and its footer stay within the viewport.
    cx.simulate_resize(size(px(1100.), px(1200.)));
    let panel = cx.update(|window, cx| {
        window.focus(&tabs.read(cx).focus.clone(), cx);
        tabs.read(cx).settings.clone()
    });
    active(&tabs, cx).update(cx, |view, cx| {
        view.pii.set_reviewing(true);
        cx.notify();
    });
    click_toolbar(cx, "Settings");
    cx.update(|window, cx| {
        assert!(panel.read(cx).open);
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("settings-dialog").is_some());
    click_toolbar(cx, "settings-category-Detection");
    click_toolbar(cx, "settings-details-fp16");
    cx.update(|_, cx| {
        assert_eq!(
            panel.read(cx).details,
            Some(settings::Model::Pii(settings::PiiModel::Fp16))
        );
        assert_eq!(panel.read(cx).category(), settings_ui::Category::Detection);
    });
    cx.simulate_resize(size(px(640.), px(480.)));
    let dialog = cx.debug_bounds("settings-dialog").unwrap();
    assert!(dialog.size.width <= px(640.));
    assert!(dialog.size.height <= px(480.));
    // The close control stays in the corner at the minimum viewport.
    let bounds = cx.debug_bounds("settings-close").unwrap();
    assert!(bounds.top() >= px(0.) && bounds.bottom() <= px(480.));
    cx.dispatch_action(settings_ui::CloseSettings);
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(!panel.read(cx).open);
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("settings-dialog").is_none());
    // A click inside keeps Settings open; one outside closes it.
    cx.simulate_resize(size(px(1100.), px(760.)));
    click_toolbar(cx, "Settings");
    let dialog = cx.debug_bounds("settings-dialog").unwrap();
    cx.simulate_click(dialog.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert!(panel.read(cx).open));
    cx.simulate_click(
        gpui::point(dialog.left() - px(20.), dialog.center().y),
        Default::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(!panel.read(cx).open, "an outside click closes Settings");
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("settings-dialog").is_none());
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
            (next - zoom - crate::document_view::ZOOM_STEP).abs() < 1e-4,
            "{keys}: {zoom} -> {next}"
        );
        zoom = next;
    }
    cx.simulate_keystrokes("secondary-0");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).zoom, 1.));
}

#[gpui::test]
fn toolbar_theme_toggle_works_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    // The sidebar's theme control cycles System → Light → Dark (ADR 0048).
    cx.update(|_, cx| assert_eq!(tabs.read(cx).theme_choice, ThemeChoice::System));
    click_toolbar(cx, "theme-toggle");
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).theme_choice, ThemeChoice::Light);
        assert_eq!(tabs.read(cx).theme.get(), Theme::Light);
    });
    cx.update(|window, cx| window.focus(&tabs.read(cx).focus.clone(), cx));
    click_toolbar(cx, "theme-toggle");
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).theme_choice, ThemeChoice::Dark);
        assert_eq!(tabs.read(cx).theme.get(), Theme::Dark);
        assert_eq!(tabs.read(cx).snapshot(cx).theme_choice, ThemeChoice::Dark);
    });
}

#[gpui::test]
fn settings_theme_choice_updates_sidebar_and_saved_session(cx: &mut TestAppContext) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    tabs.update(cx, |tabs, _| {
        tabs.ready = true;
        tabs.session_writer.set_path(Some(path.clone()));
    });
    click_toolbar(cx, "Settings");
    for (id, selected, choice, theme) in [
        (
            "settings-theme-system",
            "settings-theme-system-selected",
            ThemeChoice::System,
            Theme::Light,
        ),
        (
            "settings-theme-light",
            "settings-theme-light-selected",
            ThemeChoice::Light,
            Theme::Light,
        ),
        (
            "settings-theme-dark",
            "settings-theme-dark-selected",
            ThemeChoice::Dark,
            Theme::Dark,
        ),
    ] {
        click_toolbar(cx, id);
        draw(cx);
        assert!(cx.debug_bounds(selected).is_some());
        cx.update(|_, cx| {
            assert_eq!(tabs.read(cx).theme_choice, choice);
            assert_eq!(tabs.read(cx).theme.get(), theme);
            // The sidebar button's live tooltip follows the same choice.
            assert_eq!(tabs.read(cx).theme_tip.get(), choice);
        });
        let saved = session_store::load(&path).unwrap();
        assert_eq!(saved.theme_choice, choice);
    }
    cx.dispatch_action(settings_ui::CloseSettings);
    cx.run_until_parked();
    let saved = session_store::load(&path).unwrap();
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.cycle_theme(window, cx);
        tabs.restore(saved, window, cx);
    });
    click_toolbar(cx, "Settings");
    assert!(cx.debug_bounds("settings-theme-dark-selected").is_some());
}

#[gpui::test]
fn settings_theme_tracks_sidebar_cycle_and_updates_while_mounted(cx: &mut TestAppContext) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    for selected in [
        "settings-theme-light-selected",
        "settings-theme-dark-selected",
        "settings-theme-system-selected",
    ] {
        click_toolbar(cx, "theme-toggle");
        click_toolbar(cx, "Settings");
        assert!(cx.debug_bounds(selected).is_some(), "{selected}");
        cx.dispatch_action(settings_ui::CloseSettings);
        cx.run_until_parked();
    }
    click_toolbar(cx, "Settings");
    // The same transition also updates the mounted panel without reopening it.
    tabs.update_in(cx, |tabs, window, cx| tabs.cycle_theme(window, cx));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("settings-theme-light-selected").is_some());
}

#[gpui::test]
fn settings_theme_segments_follow_category_and_activate_with_enter_and_space(
    cx: &mut TestAppContext,
) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot_home(
        cx,
        Session {
            theme_choice: ThemeChoice::Dark,
            ..Session::default()
        },
    );
    cx.simulate_resize(size(px(640.), px(480.)));
    click_toolbar(cx, "Settings");
    draw(cx);
    for (key, choice) in [
        ("enter", ThemeChoice::System),
        ("space", ThemeChoice::Light),
        ("enter", ThemeChoice::Dark),
    ] {
        cx.simulate_keystrokes("tab");
        draw(cx);
        cx.simulate_keystrokes(key);
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse(key).unwrap(),
        });
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(tabs.read(cx).theme_choice, choice));
    }
    cx.simulate_keystrokes("shift-tab");
    draw(cx);
    cx.simulate_keystrokes("space");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("space").unwrap(),
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).theme_choice, ThemeChoice::Light));
}

#[gpui::test]
fn toolbar_preview_toggle_works_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
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
        assert_eq!(view.read(cx).preview.attachment(), Some(&path));
    });
}

#[gpui::test]
fn detached_header_routes_preview_and_document_shortcuts(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    tabs.update_in(cx, |tabs, window, cx| tabs.open_path(path, window, cx));
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let pdf = cx.update(|_, cx| view.read(cx).preview.pdf().cloned().unwrap());
    // The find button's action; Mod+F opens the Markdown find.
    pdf.update_in(cx, |pdf, window, cx| pdf.toggle_search(window, cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| pdf.read(cx).is_search_open()));
    // Composed and non-ASCII text arrives through the input handler, not key
    // codes, so IME and other layouts reach the query (ADR 0057, K7).
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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
    let mut field = None;
    let (pdf, cx) = cx.add_window_view(|_, cx| {
        let mut pdf = PdfView::new(path, Rc::new(Default::default), Rc::new(|| 1.), cx);
        field = Some(crate::preview::attach_search_field(&mut pdf, cx));
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
    let pdf = base.join("tests/fixtures/reference.pdf");
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
        assert!(view.read(cx).session.document.path.is_none());
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
fn every_menu_item_reaches_a_handler_in_a_document(cx: &mut TestAppContext) {
    let (_tabs, cx) = boot(cx, Session::default());
    cx.update(|_, cx| crate::menus::install(cx));
    cx.simulate_input("alpha beta");
    cx.run_until_parked();
    let recent = [std::path::PathBuf::from("/tmp/recent.md")];
    for (name, action) in crate::menus::items(&recent) {
        // In the window's focus path, or app-wide (Hide, Minimize).
        let available = cx.update(|window, cx| {
            window.is_action_available(action.as_ref(), cx)
                || cx.is_action_available(action.as_ref())
        });
        assert!(available, "menu item {name} has no handler");
    }
}

/// File › Open Recent follows Recents: its rows open their file, and Clear
/// Menu empties it and Recents (decision 10 of the UI polish grill).
#[gpui::test]
fn open_recent_follows_recents_and_clear_menu_empties_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["first.md", "second.md"]
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::write(&path, *name).unwrap();
            path
        })
        .collect();
    let (tabs, cx) = boot_home(cx, Session::default());
    let titles =
        |cx: &mut VisualTestContext| cx.update(|_, cx| crate::menus::open_recent_titles(cx));
    assert_eq!(titles(cx), ["Clear Menu"]);
    for path in &paths {
        cx.dispatch_action(crate::workspace::OpenRecent(path.clone()));
        cx.run_until_parked();
    }
    assert_eq!(tab_names(&tabs, cx), ["first.md", "second.md"]);
    assert_eq!(titles(cx), ["second.md", "first.md", "Clear Menu"]);
    cx.dispatch_action(crate::workspace::ClearRecentFiles);
    cx.run_until_parked();
    assert_eq!(titles(cx), ["Clear Menu"]);
    cx.update(|_, cx| assert!(tabs.read(cx).shell.recents.files.is_empty()));
}

#[gpui::test]
fn edit_menu_reaches_the_find_field(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_input("alpha beta");
    cx.dispatch_action(FindMarkdown);
    cx.run_until_parked();
    cx.simulate_input("alpha");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("beta".into()));
    cx.dispatch_action(mdoc_editor::SelectAll);
    cx.dispatch_action(mdoc_editor::Paste);
    cx.run_until_parked();
    let view = active(&tabs, cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).markdown_search.read(cx).value(), "beta"));
    cx.dispatch_action(mdoc_editor::SelectAll);
    cx.dispatch_action(mdoc_editor::Cut);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).markdown_search.read(cx).value(), ""));
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("beta")
    );
    // The document is untouched.
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), "alpha beta"));
}

#[gpui::test]
#[ignore = "host CPU measurements; run serially on an idle machine"]
fn tabs_host_performance(cx: &mut TestAppContext) {
    use std::time::Instant;
    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    }
    fn report(label: &str, values: &mut [f64]) {
        values.sort_by(f64::total_cmp);
        eprintln!(
            "TABS_PERF {label} median_ms={:.3} p95_ms={:.3}",
            values[values.len() / 2],
            values[(values.len() * 95).div_ceil(100) - 1]
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pdf = base.join("tests/fixtures/reference.pdf");
    let docx = base.join("tests/fixtures/docx-preview/comments.docx");
    let mut session = Session::default();
    for index in 0..24 {
        let path = dir.path().join(format!("note-{index}.md"));
        let source = if index % 3 == 0 {
            "# Heading\n\nalpha **beta** words for a long document.\n\n".repeat(2500)
        } else {
            "# Short\n\nhello world\n".repeat(30)
        };
        std::fs::write(&path, source).unwrap();
        session.tabs.push(TabRecord {
            markdown: path,
            attachment: match index % 3 {
                0 => None,
                1 => Some(pdf.clone()),
                _ => Some(docx.clone()),
            },
            preview_visible: index % 3 != 0,
            ..TabRecord::default()
        });
    }
    let start = Instant::now();
    let (tabs, cx) = boot(cx, session);
    cx.simulate_resize(size(px(1280.), px(800.)));
    draw(cx);
    eprintln!(
        "TABS_PERF restore_24_tabs_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.
    );
    let ids = cx.update(|_, cx| {
        tabs.read(cx)
            .tabs
            .iter()
            .map(|tab| tab.id())
            .collect::<Vec<_>>()
    });
    cx.update(|_, cx| {
        assert_eq!(
            tabs.read(cx)
                .tabs
                .iter()
                .filter(|tab| tab.view().is_some())
                .count(),
            1
        )
    });
    for id in &ids[..6] {
        tabs.update_in(cx, |tabs, window, cx| tabs.activate(*id, window, cx));
        cx.run_until_parked();
        draw(cx);
    }
    let mut switching = Vec::new();
    let mut handlers = Vec::new();
    let mut frames = Vec::new();
    let mut long_frames = Vec::new();
    let mut short_frames = Vec::new();
    for round in 0..24 {
        let id = ids[round % 6];
        let start = Instant::now();
        let handler = tabs.update_in(cx, |tabs, window, cx| {
            let start = Instant::now();
            tabs.activate(id, window, cx);
            start.elapsed().as_secs_f64() * 1000.
        });
        handlers.push(handler);
        switching.push(start.elapsed().as_secs_f64() * 1000.);
        let start = Instant::now();
        draw(cx);
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        frames.push(elapsed);
        if round % 3 == 0 {
            long_frames.push(elapsed);
        } else {
            short_frames.push(elapsed);
        }
    }
    report("switch_handler", &mut handlers);
    report("switch_with_implicit_test_frame", &mut switching);
    report("switch_frame", &mut frames);
    report("long_markdown_frame", &mut long_frames);
    report("short_markdown_with_preview_frame", &mut short_frames);
    // Same document, same viewport, no tab change: distinguish inherited editor
    // layout cost from the sidebar/activation overhead.
    tabs.update_in(cx, |tabs, window, cx| tabs.activate(ids[0], window, cx));
    let mut unchanged = Vec::new();
    for _ in 0..8 {
        let start = Instant::now();
        draw(cx);
        unchanged.push(start.elapsed().as_secs_f64() * 1000.);
    }
    report("unchanged_long_markdown_frame", &mut unchanged);
    let view = active(&tabs, cx);
    eprintln!(
        "TABS_PERF unchanged_long_editor_width={:?}",
        view.read_with(cx, |view, cx| view
            .editor
            .read(cx)
            .layout_width()
            .map(f32::from))
    );
    assert!(
        handlers.iter().all(|ms| *ms < 16.7),
        "tab activation alone exceeded one CPU frame"
    );
    let mut snapshots = Vec::new();
    for _ in 0..30 {
        let start = Instant::now();
        cx.update(|_, cx| {
            let _ = tabs.read(cx).snapshot(cx);
        });
        snapshots.push(start.elapsed().as_secs_f64() * 1000.);
    }
    report("metadata_snapshot_24_tabs", &mut snapshots);
    // Repeated DOCX pairs exercise temporary files and retained viewer release.
    eprintln!(
        "TABS_PERF before_close_cycles_rss_kib={:?}",
        crate::perf_tests::rss_kib()
    );
    for cycle in 0..5 {
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.open_path(docx.clone(), window, cx)
        });
        cx.run_until_parked();
        let id = cx.update(|_, cx| tabs.read(cx).tabs.active());
        draw(cx);
        let (weak, backing) = cx.update(|_, cx| {
            let view = tabs.read(cx).active_view().unwrap();
            let view = view.read(cx);
            (
                view.preview.pdf().unwrap().downgrade(),
                view.preview.docx().unwrap().pdf_path.clone(),
            )
        });
        tabs.update_in(cx, |tabs, window, cx| tabs.remove(id, window, cx));
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
        assert!(crate::test_support::removed(&backing));
        eprintln!(
            "TABS_PERF close_cycle={cycle} rss_kib={:?}",
            crate::perf_tests::rss_kib()
        );
    }
    eprintln!("TABS_PERF close_cycles=5 all_viewers_and_docx_files_released=true");
}

#[gpui::test]
fn menu_routes_save_save_as_and_open_to_the_active_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let first_path = dir.path().join("first.md");
    let copy_path = dir.path().join("copy.md");
    let other_path = dir.path().join("other.md");
    std::fs::write(&other_path, "other document").unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    view.update(cx, |v, cx| {
        v.editor.update(cx, |e, cx| e.set_text("first version", cx))
    });
    cx.run_until_parked();
    more(cx, "more-save");
    cx.simulate_new_path_selection(|_| Some(first_path.clone()));
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(&first_path).unwrap(),
        "first version"
    );
    view.update(cx, |v, cx| {
        v.editor
            .update(cx, |e, cx| e.set_text("updated version", cx))
    });
    cx.run_until_parked();
    more(cx, "more-save");
    assert_eq!(
        std::fs::read_to_string(&first_path).unwrap(),
        "updated version"
    );
    more(cx, "more-save-as");
    cx.simulate_new_path_selection(|_| Some(copy_path.clone()));
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(&copy_path).unwrap(),
        "updated version"
    );
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).session.document.path.as_ref(),
            Some(&dunce::canonicalize(&copy_path).unwrap())
        )
    });
    // Open has no header button: ⌘O, Home and the palette (ADR 0048).
    cx.dispatch_action(Open);
    cx.run_until_parked();
    cx.simulate_path_prompt_response(|_| Some(vec![other_path]));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let opened = tabs.read(cx).active_view().unwrap();
        assert_ne!(opened.entity_id(), view.entity_id());
        assert_eq!(opened.read(cx).editor.read(cx).text(), "other document");
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
                // One header row (ADR 0048): ⋯ and the primary action inside it.
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
    let pdf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
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
fn settings_keyboard_traversal_stays_in_dialog_and_restores_opener(cx: &mut TestAppContext) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(640.), px(480.)));
    let opener = cx.update(|window, cx| {
        let opener = tabs.read(cx).settings_focus.clone();
        window.focus(&opener, cx);
        opener
    });
    click_toolbar(cx, "Settings");
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    for _ in 0..25 {
        cx.simulate_keystrokes("tab");
        cx.update(|window, cx| {
            assert!(gpui::Focusable::focus_handle(panel.read(cx), cx).contains_focused(window, cx));
            assert_eq!(panel.read(cx).category(), settings_ui::Category::General);
        });
    }
    cx.simulate_keystrokes("shift-tab escape");
    cx.update(|window, cx| {
        assert!(!panel.read(cx).open);
        assert!(opener.is_focused(window));
    });
}

#[gpui::test]
fn settings_categories_use_one_tab_stop_and_arrow_keys(cx: &mut TestAppContext) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    click_toolbar(cx, "Settings");
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let category_focus = cx.update(|window, cx| window.focused(cx).unwrap());
    for category in [
        settings_ui::Category::Detection,
        settings_ui::Category::Recognition,
        settings_ui::Category::Spelling,
        settings_ui::Category::Cases,
        settings_ui::Category::About,
        settings_ui::Category::General,
    ] {
        key(cx, "down");
        cx.update(|window, cx| {
            assert_eq!(panel.read(cx).category(), category);
            assert_eq!(window.focused(cx), Some(category_focus.clone()));
        });
    }
    key(cx, "up");
    cx.update(|_, cx| assert_eq!(panel.read(cx).category(), settings_ui::Category::About));
    key(cx, "down");
    key(cx, "shift-tab");
    let close_focus = cx.update(|window, cx| window.focused(cx).unwrap());
    assert_ne!(close_focus, category_focus);
    activate_control(cx);
    cx.update(|_, cx| assert!(!panel.read(cx).open));
    click_toolbar(cx, "Settings");
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(category_focus.clone())));
    key(cx, "tab");
    let content_focus = cx.update(|window, cx| window.focused(cx).unwrap());
    assert_ne!(content_focus, category_focus);
    assert_ne!(content_focus, close_focus);
    key(cx, "shift-tab");
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(category_focus)));
}

#[gpui::test]
fn settings_model_and_update_deep_links_choose_their_categories(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    for model in settings::Model::ALL {
        panel.update_in(cx, |panel, window, cx| {
            panel.show_section(model, window, cx)
        });
        draw(cx);
        let expected = match model {
            settings::Model::Ocr(_) => settings_ui::Category::Recognition,
            settings::Model::Pii(_) => settings_ui::Category::Detection,
        };
        cx.update(|_, cx| {
            assert!(panel.read(cx).open);
            assert_eq!(panel.read(cx).category(), expected, "{model:?}");
        });
    }
    panel.update_in(cx, |panel, window, cx| panel.show_updates(window, cx));
    cx.update(|_, cx| assert_eq!(panel.read(cx).category(), settings_ui::Category::General));
    panel.update_in(cx, |panel, window, cx| {
        panel.show_category(settings_ui::Category::About, window, cx);
        panel.close(window, cx);
        panel.show(window, cx);
    });
    cx.update(|_, cx| assert_eq!(panel.read(cx).category(), settings_ui::Category::General));
}

#[gpui::test]
fn narrow_original_switch_and_divider_preserve_source_and_session(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference.pdf");
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

    // Spelling lives in the header's ⋯ menu (ADR 0048).
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

#[gpui::test]
fn dictionary_view_searches_adds_removes_with_undo_and_stays_virtualized(cx: &mut TestAppContext) {
    cx.update(settings_ui::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let shared = cx.update(|_, cx| panel.read(cx).shared.clone());
    let words: std::collections::BTreeSet<String> =
        (0..300).map(|i| format!("word{i:03}")).collect();
    cx.update(|_, cx| crate::settings_ui::dictionary::set_words(&shared, words, cx));
    click_toolbar(cx, "Settings");
    click_toolbar(cx, "settings-category-Spelling");
    let scroll = cx.debug_bounds("settings-scroll").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroll.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-600.))),
        ..Default::default()
    });
    draw(cx);
    click_toolbar(cx, "manage-dictionary");
    draw(cx);
    // Only the visible rows exist.
    assert!(cx.debug_bounds("remove-word-0").is_some());
    assert!(cx.debug_bounds("remove-word-250").is_none());

    let query = cx.update(|_, cx| panel.read(cx).dictionary.as_ref().unwrap().query.clone());
    let type_query = |cx: &mut VisualTestContext, text: &str| {
        query.update(cx, |query, cx| {
            query.set_value(text.into(), cx);
            cx.emit(markdown_search::SearchInputEvent::Changed);
        });
        draw(cx);
    };
    type_query(cx, "word29");
    assert!(
        cx.debug_bounds("remove-word-9").is_some(),
        "word290..word299"
    );
    assert!(cx.debug_bounds("remove-word-10").is_none());
    // Not itself a stored word, so it can be added.
    assert!(cx.debug_bounds("add-dictionary-word").is_some());
    type_query(cx, "word290");
    assert!(
        cx.debug_bounds("add-dictionary-word").is_none(),
        "exact match"
    );

    // A new word: Add, then the field clears.
    type_query(cx, "Tebriz");
    click_toolbar(cx, "add-dictionary-word");
    cx.update(|_, cx| {
        assert!(shared.borrow().words.words.contains("Tebriz"));
        assert!(query.read(cx).value().is_empty());
    });

    // Remove the first word, then Undo restores it.
    type_query(cx, "");
    click_toolbar(cx, "remove-word-0");
    cx.update(|_, _| assert!(!shared.borrow().words.words.contains("Tebriz")));
    click_toolbar(cx, "undo-remove-word");
    cx.update(|_, _| assert!(shared.borrow().words.words.contains("Tebriz")));

    // The back control returns to Spelling; Escape from the drill-in closes Settings.
    click_toolbar(cx, "close-dictionary");
    cx.update(|_, cx| {
        assert!(panel.read(cx).dictionary.is_none());
        assert!(panel.read(cx).open);
        assert_eq!(panel.read(cx).category(), settings_ui::Category::Spelling);
    });
    draw(cx);
    assert!(cx.debug_bounds("manage-dictionary").is_some());
    click_toolbar(cx, "manage-dictionary");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!panel.read(cx).open));
    draw(cx);
    assert!(cx.debug_bounds("settings-dialog").is_none());
}

#[gpui::test]
fn ai_reply_round_trip_restores_in_a_new_tab_and_reapplies_aliases(cx: &mut TestAppContext) {
    use mdoc_pii::Category;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contract.md");
    let source = "Anna Petrova met Northwind Traders Ltd.";
    std::fs::write(&path, source).unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path], window, cx)
    });
    cx.run_until_parked();
    let first = active(&tabs, cx);
    let first_id = cx.update(|_, cx| tabs.read(cx).tabs.active());
    first.update_in(cx, |view, window, cx| {
        view.pii.set_reviewing(true);
        view.pii
            .review_mut()
            .add_manual(source, 0..12, Category::Person)
            .unwrap();
        view.pii
            .review_mut()
            .add_manual(source, 17..38, Category::Organization)
            .unwrap();
        view.apply_all_pii(&PiiApplyAll, window, cx);
        assert_eq!(view.editor.read(cx).text(), "PERSON_1 met ORG_1.");
    });
    cx.run_until_parked();

    // Inspector (ADR 0048, C): Share in the header, Copy for AI in the tab.
    click_toolbar(cx, "primary-share");
    click_toolbar(cx, "share-copy-for-ai");
    cx.update(|_, cx| {
        let copied = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(copied.starts_with("Note: names and other identifying details"));
        assert!(copied.contains("such as PERSON_1 or ORG_1."));
        assert!(copied.ends_with("---\n\nPERSON_1 met ORG_1."));
        assert!(!copied.contains("Anna"), "the notice never names originals");
    });

    let reply = "**PERSON_1** owes ORG_1; person 1 and PERSON_7 disagree.";
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(reply.into()));
    more(cx, "more-paste-reply");
    let restored = active(&tabs, cx);
    cx.update(|_, cx| {
        assert_ne!(tabs.read(cx).tabs.active(), first_id);
        let view = restored.read(cx);
        assert_eq!(
            view.editor.read(cx).text(),
            "**Anna Petrova** owes Northwind Traders Ltd; Anna Petrova and PERSON_7 disagree."
        );
        assert_eq!(view.display_name(), "Restored – contract.md");
        assert!(view.dirty(cx), "a restored reply is unsaved work");
        assert_eq!(view.pii.review().remaining(), 3);
        assert_eq!(view.unresolved_tokens(cx).len(), 1);
        assert_eq!(
            first.read(cx).editor.read(cx).text(),
            "PERSON_1 met ORG_1.",
            "the source tab is untouched"
        );
    });
    draw(cx);
    assert!(cx.debug_bounds("restore-summary").is_some());

    // Applying the proposals re-pseudonymizes with the source's aliases.
    restored.update_in(cx, |view, window, cx| {
        view.apply_all_pii(&PiiApplyAll, window, cx)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            restored.read(cx).editor.read(cx).text(),
            "**PERSON_1** owes ORG_1; PERSON_1 and PERSON_7 disagree."
        );
    });
    // Undo returns to the restored text, then to the reply as pasted.
    for expected in [
        "**Anna Petrova** owes Northwind Traders Ltd; Anna Petrova and PERSON_7 disagree.",
        reply,
    ] {
        cx.update(|window, cx| {
            window.focus(&restored.read(cx).editor.read(cx).focus_handle(cx), cx)
        });
        cx.dispatch_action(mdoc_editor::Undo);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(restored.read(cx).editor.read(cx).text(), expected));
    }

    // Unsaved restored replies are not written to the session manifest.
    cx.update(|_, cx| {
        let session = tabs.read(cx).snapshot(cx);
        assert_eq!(session.tabs.len(), 1);
    });
}

#[gpui::test]
fn paste_ai_response_needs_applied_aliases_and_clipboard_text(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let view = active(&tabs, cx);
    cx.simulate_input("PERSON_1 met ORG_1.");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("PERSON_1".into()));
    view.update_in(cx, |view, window, cx| {
        view.paste_ai_response(&PasteAiResponse, window, cx);
        assert!(
            view.error
                .as_deref()
                .unwrap()
                .contains("Apply replacements")
        );
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(tabs.read(cx).tabs.len(), 1));
}

#[gpui::test]
fn chrome_history_undoes_toolbar_apply_and_redoes_without_editor_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    cx.update(|_, cx| bind_app_keys(cx));
    let view = active(&tabs, cx);
    let source = "Anna Petrova met Northwind Traders Ltd.";
    let applied = "PERSON_1 met ORG_1.";
    view.update_in(cx, |view, window, cx| {
        view.editor
            .update(cx, |editor, cx| editor.undo(&mdoc_editor::Undo, window, cx));
    });
    cx.run_until_parked();
    click_toolbar(cx, "primary-apply");
    cx.update(|window, cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), applied);
        assert!(
            window
                .context_stack()
                .iter()
                .any(|context| context.contains("UiControl"))
        );
        assert!(
            !view
                .read(cx)
                .editor
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
    let (undo, redo) = if cfg!(target_os = "macos") {
        ("cmd-z", "cmd-shift-z")
    } else {
        ("ctrl-z", "ctrl-shift-z")
    };
    key(cx, undo);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), source);
        assert_eq!(view.read(cx).pii.review().remaining(), 2);
    });
    key(cx, redo);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).editor.read(cx).text(), applied);
        assert_eq!(view.read(cx).pii.review().remaining(), 0);
    });
    if !cfg!(target_os = "macos") {
        key(cx, undo);
        key(cx, "ctrl-y");
        cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), applied));
    }
}

#[gpui::test]
fn chrome_history_targets_the_active_tab_and_leaves_home_and_settings_alone(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    cx.update(|_, cx| bind_app_keys(cx));
    let first = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.new_tab(window, cx));
    cx.run_until_parked();
    let second = active(&tabs, cx);
    second.update(cx, |view, cx| {
        view.editor
            .update(cx, |editor, cx| editor.replace_range(0..0, "second", cx));
    });
    cx.run_until_parked();
    // The sidebar's toggle is outside DocumentView's dispatch subtree.
    click_toolbar(cx, "sidebar-toggle");
    let undo = if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    };
    key(cx, undo);
    cx.update(|_, cx| {
        assert_eq!(second.read(cx).editor.read(cx).text(), "");
        assert_eq!(first.read(cx).editor.read(cx).text(), "PERSON_1 met ORG_1.");
    });
    key(
        cx,
        if cfg!(target_os = "macos") {
            "cmd-shift-z"
        } else {
            "ctrl-shift-z"
        },
    );
    click_toolbar(cx, "sidebar-toggle");
    click_toolbar(cx, "sidebar-home");
    key(cx, undo);
    cx.update(|_, cx| assert_eq!(second.read(cx).editor.read(cx).text(), "second"));
    tabs.update_in(cx, |tabs, window, cx| {
        let id = tabs
            .tabs
            .iter()
            .find(|tab| tab.view() == Some(&second))
            .unwrap()
            .id();
        tabs.activate(id, window, cx);
        tabs.show_settings(window, cx);
    });
    cx.run_until_parked();
    key(cx, undo);
    cx.update(|_, cx| assert_eq!(second.read(cx).editor.read(cx).text(), "second"));
}

/// Paste text puts the clipboard straight into the new tab (issue #300):
/// a dispatched Paste reached no editor and left an empty Untitled tab.
#[gpui::test]
fn paste_text_fills_the_new_tab(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let texts = |tabs: &Entity<Workspace>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            tabs.read(cx)
                .tabs
                .iter()
                .map(|tab| {
                    tab.view()
                        .unwrap()
                        .read(cx)
                        .editor
                        .read(cx)
                        .text()
                        .to_owned()
                })
                .collect::<Vec<_>>()
        })
    };
    // From Home's button, with a Windows line ending to normalize.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "Two lines
Вторая строка"
            .into(),
    ));
    click_toolbar(cx, "home-paste");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| !tabs.read(cx).showing_home(cx)));
    assert_eq!(
        texts(&tabs, cx),
        ["Two lines
Вторая строка"]
    );
    // From the palette over that document: a second tab, the first untouched.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("Second paste".into()));
    cx.simulate_keystrokes("secondary-k");
    cx.simulate_input("paste text as new tab");
    draw(cx);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        texts(&tabs, cx),
        [
            "Two lines
Вторая строка",
            "Second paste"
        ]
    );
    assert_eq!(
        active(&tabs, cx).read_with(cx, |view, cx| view.editor.read(cx).text().to_owned()),
        "Second paste"
    );
    // A blank clipboard opens no tab and says why.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "  
"
        .into(),
    ));
    tabs.update_in(cx, |tabs, window, cx| tabs.paste_as_new_tab(window, cx));
    cx.run_until_parked();
    assert_eq!(texts(&tabs, cx).len(), 2);
}

#[gpui::test]
fn home_clipboard_actions_use_the_active_documents_mapping(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let source = active(&tabs, cx);
    click_toolbar(cx, "sidebar-home");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("Call Anna Petrova.".into()));
    click_toolbar(cx, "home-clipboard");
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).showing_home(cx));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Call PERSON_1.")
        );
        assert!(source.read(cx).clipboard_notice.text().is_some());
    });
    click_toolbar(cx, "sidebar-home");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
        "PERSON_1 owes ORG_1.".into(),
    ));
    click_toolbar(cx, "home-restore");
    let restored = active(&tabs, cx);
    assert_ne!(source, restored);
    cx.update(|_, cx| {
        assert!(!tabs.read(cx).showing_home(cx));
        assert_eq!(
            restored.read(cx).editor.read(cx).text(),
            "Anna Petrova owes Northwind Traders Ltd."
        );
    });
}

#[gpui::test]
fn command_palette_filters_and_runs_actions(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    draw(cx);
    // ⌘K opens it from the document (ADR 0048).
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_some());
    cx.simulate_input("home");
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_none());
    assert!(cx.debug_bounds("home").is_some());
    // From Home, Esc closes it again.
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_some());
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("command-palette").is_none());
    cx.update(|_, cx| assert!(tabs.read(cx).shell.palette.is_none()));
    // View actions without a menu bar on Windows are in the palette too.
    let visible = tabs.read_with(cx, |tabs, _| tabs.sidebar.visible());
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    cx.simulate_input("toggle sidebar");
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    tabs.read_with(cx, |tabs, _| assert_eq!(tabs.sidebar.visible(), !visible));
}

/// The palette's theme row names the theme it switches to (U14), and running
/// it moves the name on to the next one.
#[gpui::test]
fn command_palette_theme_row_names_its_target(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    for (now, row, next) in [
        (
            ThemeChoice::System,
            "Switch to light theme",
            ThemeChoice::Light,
        ),
        (
            ThemeChoice::Light,
            "Switch to dark theme",
            ThemeChoice::Dark,
        ),
        (
            ThemeChoice::Dark,
            "Switch to system theme",
            ThemeChoice::System,
        ),
    ] {
        tabs.read_with(cx, |tabs, cx| {
            assert_eq!(tabs.theme_choice, now);
            let actions = tabs.palette_section("Actions", cx);
            assert!(actions.iter().any(|label| label == row), "{actions:?}");
            assert!(
                !actions
                    .iter()
                    .any(|label| label.starts_with("Switch theme"))
            );
        });
        cx.simulate_keystrokes("secondary-k");
        cx.simulate_input(row);
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        cx.update(|_, cx| assert_eq!(tabs.read(cx).theme_choice, next));
    }
}

#[gpui::test]
fn command_palette_routes_every_settings_category(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let labels = tabs.read_with(cx, |tabs, cx| tabs.palette_section("Go to", cx));
    for category in settings_ui::Category::ALL {
        assert!(labels.contains(&format!("Settings › {}", category.label())));
        cx.simulate_keystrokes("secondary-k");
        cx.simulate_input(&format!("settings {}", category.label()));
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        cx.update(|_, cx| {
            assert!(tabs.read(cx).shell.palette.is_none());
            assert!(panel.read(cx).open);
            assert_eq!(panel.read(cx).category(), category);
        });
        panel.update_in(cx, |panel, window, cx| panel.close(window, cx));
        draw(cx);
    }
}

/// Public synthetic input through the real editor/review/case host. Timings are
/// observations until release/native budgets are qualified (ADR 0049).
#[gpui::test]
#[ignore = "review/diagnostics/compare/case CPU matrix; run serially on an idle machine"]
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
    tabs.update(cx, |tabs, _| {
        tabs.use_case_store(mdoc_store::cases::Store::with_key(
            dir.path().join("cases"),
            &[3; 32],
        ));
    });
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
        view.inspector_share = false;
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
    let first = open_applied(&tabs, cx, dir.path(), "first.md", &text, &wordings);
    let first_view = active(&tabs, cx);
    tabs.update_in(cx, |tabs, window, cx| tabs.create_case(first, window, cx));
    cx.run_until_parked();
    let case = tabs.read_with(cx, |tabs, _| tabs.case_coordinator.cases()[0].id.clone());
    for name in ["second.md", "third.md"] {
        let member = open_applied(&tabs, cx, dir.path(), name, &text, &wordings);
        tabs.update_in(cx, |tabs, window, cx| {
            tabs.join_case(member, &case, window, cx)
        });
        cx.run_until_parked();
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
            view.inspector_share = false;
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
                !view.inspector_share,
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
    tabs.update(cx, |tabs, cx| tabs.case_changed(first, cx));
    cx.run_until_parked();
    draw(cx);
    eprintln!(
        "REVIEW_PERF case_fanout_3_members_ms={:.3} rss_kib={:?}",
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
                    .case
                    .link()
                    .is_some_and(|link| link.id == case)
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
    cx.update(shell::bind_keys);
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let view = active(&tabs, cx);
    view.update_in(cx, |view, window, cx| {
        if !view.replacements_open() {
            view.toggle_replacements_panel(window, cx);
        }
        // Apply all leaves the inspector on Share; the list is on Review.
        view.inspector_share = false;
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

/// The palette lists the document's headings; choosing one moves the caret
/// to its line and scrolls it into view, leaving Undo as it was.
#[gpui::test]
fn palette_goes_to_a_heading(cx: &mut TestAppContext) {
    cx.update(shell::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(600.)));
    let view = active(&tabs, cx);
    let filler = "Paragraph text.\n\n".repeat(60);
    let text = format!("# Overview\n\n{filler}## Termination clause\n\n{filler}## Fees\n");
    view.update_in(cx, |view, _, cx| {
        view.editor.update(cx, |e, cx| e.set_text(&text, cx));
        view.editor.update(cx, |e, cx| e.set_cursor(text.len(), cx));
    });
    tabs.update_in(cx, |tabs, window, cx| tabs.focus_active(window, cx));
    draw(cx);
    // One typed character is the Undo step to keep.
    cx.simulate_input("x");
    draw(cx);
    let rows = cx.update(|_, cx| tabs.read(cx).palette_section("Go to heading", cx));
    assert_eq!(rows, ["Overview", "Termination clause", "Fees"]);
    // A narrow window showing Source switches back to the Markdown pane.
    view.update(cx, |view, _| view.original_selected = true);
    cx.simulate_keystrokes("secondary-k");
    draw(cx);
    cx.simulate_input("termination");
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    draw(cx);
    let target = text.find("## Termination").unwrap();
    assert!(!cx.update(|_, cx| view.read(cx).original_selected));
    assert!(editor_focused(&tabs, cx));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.editor.read(cx).cursor(), target);
        let caret = view.editor.read(cx).caret_bounds().unwrap();
        let viewport = view.scroll.bounds();
        assert!(
            caret.top() >= viewport.top() && caret.bottom() <= viewport.bottom(),
            "heading {caret:?} outside {viewport:?}"
        );
    });
    // Undo still removes the typed character and nothing else.
    cx.dispatch_action(mdoc_editor::Undo);
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).editor.read(cx).text(), text));
}

#[gpui::test]
fn home_accepts_dropped_files_and_rejects_unsupported(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
    let unknown = dir.path().join("unknown.bin");
    for path in [&a, &b, &unknown] {
        std::fs::write(path, "x").unwrap();
    }
    let (tabs, cx) = boot_home(cx, Session::default());
    draw(cx);
    let center = cx.debug_bounds("home-drop").unwrap().center();

    // A drag anywhere over Home lights the drop zone and counts the files.
    drag_files(cx, std::slice::from_ref(&unknown), center);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).shell.drop, Some(0)));
    drop_files(cx, center);
    cx.update(|_, cx| {
        assert_eq!(tabs.read(cx).shell.drop, None);
        assert_eq!(tabs.read(cx).tabs.len(), 1);
    });

    drag_files(cx, &[a.clone(), b.clone(), unknown.clone()], center);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).shell.drop, Some(2)));
    drop_files(cx, center);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
    cx.update(|_, cx| {
        assert!(
            tabs.read(cx)
                .notices
                .message()
                .unwrap()
                .contains("unknown.bin")
        );
    });

    // A document is no Home drop target.
    assert!(cx.debug_bounds("home-drop").is_none());
    drag_files(cx, std::slice::from_ref(&b), center);
    cx.update(|_, cx| assert_eq!(tabs.read(cx).shell.drop, None));
    drop_files(cx, center);
    assert_eq!(tab_names(&tabs, cx), ["a.md", "b.md"]);
}

#[gpui::test]
fn pii_download_retains_card_focus(cx: &mut TestAppContext) {
    download_retains_card_focus(cx, settings::Model::Pii(settings::PiiModel::Fp16));
}

/// Home's "Open a case…" text ends where "Documents stay on this Mac" above
/// it ends; only its hover padding runs past (U25).
#[gpui::test]
fn home_open_a_case_ends_under_the_local_note(cx: &mut TestAppContext) {
    let (_tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    draw(cx);
    let note = cx.debug_bounds("empty-page-local").unwrap();
    let link = cx.debug_bounds("home-open-case").unwrap();
    assert!(link.top() > note.bottom(), "the link sits under the note");
    assert_eq!(link.right() - px(8.), note.right());
}

/// Tooltips name the key that does the same (U16): the sidebar's toggle,
/// Back and New tab, and the toolbar's Pseudonymize. Home's "Documents stay
/// on this Mac" explains itself above the note, clear of the buttons beside
/// it.
#[gpui::test]
fn tooltips_name_their_keys_and_the_home_note_opens_above(cx: &mut TestAppContext) {
    use crate::{shortcuts::tip, workspace::navigation_ui::Back};
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    draw(cx);
    let hover = |cx: &mut VisualTestContext, id: &'static str| {
        let at = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("{id}"))
            .center();
        cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(600));
        cx.run_until_parked();
        draw(cx);
    };
    let shown = |cx: &mut VisualTestContext, text: String| {
        let selector: &'static str = Box::leak(format!("tooltip-{text}").into_boxed_str());
        assert!(cx.debug_bounds(selector).is_some(), "no tooltip “{text}”");
    };

    // Home's note: the explanation sits above it, right-aligned, and covers
    // none of the buttons beside it.
    hover(cx, "empty-page-local");
    let note = cx.debug_bounds("empty-page-local").unwrap();
    let machine = if cfg!(target_os = "macos") {
        "this Mac"
    } else {
        "this computer"
    };
    let detail = format!(
        "tooltip-Detection, conversion and saved cases run on {machine}. mdoc goes online \
         only to check for updates and to download models you choose."
    );
    let bubble = cx
        .debug_bounds(Box::leak(detail.into_boxed_str()))
        .expect("the note explains itself on hover");
    assert!(bubble.bottom() <= note.top(), "{bubble:?} over {note:?}");
    assert!((bubble.right() - note.right()).abs() < px(1.));
    let clipboard = cx.debug_bounds("home-clipboard").unwrap();
    assert!(
        !bubble.intersects(&clipboard),
        "{bubble:?} covers {clipboard:?}"
    );

    hover(cx, "sidebar-toggle");
    shown(cx, tip("Collapse sidebar", &ToggleSidebar));
    hover(cx, "sidebar-new");
    shown(cx, tip("New Markdown tab", &New));
    // A tab to go back from.
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.new_tab(window, cx);
    });
    cx.simulate_input("alpha beta");
    draw(cx);
    let back = tabs.read_with(cx, |tabs, cx| tabs.navigation_label(true, cx));
    hover(cx, "workspace-back");
    shown(cx, tip(back, &Back));
    hover(cx, "Pseudonymize");
    shown(
        cx,
        tip("Find names to replace", &crate::pii::ui::Pseudonymize),
    );
}
