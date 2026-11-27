//! Settings through the window: the panel's keys and categories, the theme,
//! the dictionary, the model card, automatic checks, and Categories to detect
//! as the default for documents outside a Project and each new Project's start.
use super::*;
use mdoc_pii::Category;
use mdoc_store::settings;

/// Tick or untick `category` in Settings and save, as a chip click does.
fn set_default(tabs: &Entity<Workspace>, cx: &mut VisualTestContext, category: Category, on: bool) {
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    panel.update(cx, |panel, cx| panel.set_detects(category, on, cx));
    cx.run_until_parked();
}

fn projects(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<mdoc_vault::Project> {
    cx.update(|_, cx| {
        let w = tabs.read(cx);
        (w.projects.summaries().into_iter())
            .filter_map(|summary| w.projects.project(&summary.id).cloned())
            .collect()
    })
}

fn originals(view: &Entity<DocumentView>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        view.read(cx)
            .pii
            .review()
            .variants()
            .iter()
            .map(|v| v.original.to_string())
            .collect()
    })
}

/// Scan proposals for `names` in the active tab, as a finished scan adds them.
fn detect(view: &Entity<DocumentView>, cx: &mut VisualTestContext, names: &[(&str, Category)]) {
    view.update(cx, |view, cx| {
        let text = view.editor.read(cx).text().to_owned();
        let detections = names
            .iter()
            .map(|(name, category)| {
                let start = text.find(name).unwrap();
                mdoc_pii::Detection {
                    range: start..start + name.len(),
                    category: *category,
                    score: 0.9,
                    recognizer: mdoc_pii::Recognizer::Model,
                }
            })
            .collect();
        view.pii.review_mut().ingest(&text, detections).unwrap();
        view.review_changed(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_saved_mapping_project_starts_from_the_default_and_keeps_its_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    set_default(&tabs, cx, Category::Address, false);
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = new_project(&tabs, cx, tab);
    assert_eq!(projects(&tabs, cx)[0].skip, ["ADDRESS"]);
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).scan_skip(), [Category::Address].into());
    });

    // A later Settings change leaves the existing Project and its member
    // alone.
    set_default(&tabs, cx, Category::Address, true);
    set_default(&tabs, cx, Category::Email, false);
    assert_eq!(projects(&tabs, cx)[0].skip, ["ADDRESS"]);
    assert_eq!(vault(&tabs, cx).project(&id).unwrap().skip, ["ADDRESS"]);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).scan_skip(), [Category::Address].into());
    });
    // Leaving the Project hands the tab back to the default.
    move_tab(&tabs, cx, tab, None);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).scan_skip(), [Category::Email].into());
    });
}

#[gpui::test]
fn a_folder_opened_as_a_project_starts_from_the_default(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Matter");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("a.md"), "Text").unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    set_default(&tabs, cx, Category::Phone, false);
    set_default(&tabs, cx, Category::Bank, false);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![folder], window, cx)
    });
    cx.run_until_parked();
    draw(cx);
    click_toolbar(cx, "folder-primary");
    cx.run_until_parked();
    let created = projects(&tabs, cx);
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].name, "Matter");
    assert_eq!(created[0].skip, ["PHONE", "BANK"]);
    // Its member scans with the Project's list.
    let view = active(&tabs, cx);
    cx.update(|_, cx| {
        assert!(view.read(cx).project.link().is_some());
        assert_eq!(
            view.read(cx).scan_skip(),
            [Category::Phone, Category::Bank].into()
        );
    });
}

#[gpui::test]
fn unticking_withdraws_unapplied_proposals_only_in_tabs_without_a_project(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx) = boot(cx, Session::default());
    let text = "Anna Petrova met Bob Day and Cara Lin at 1 Main St.";
    // A Project's document: its Project decides, whatever Settings says.
    let member = open_applied(
        &tabs,
        cx,
        dir.path(),
        "member.md",
        text,
        &[("Anna Petrova", Category::Person)],
    );
    new_project(&tabs, cx, member);
    let member_view = active(&tabs, cx);
    detect(&member_view, cx, &[("Bob Day", Category::Person)]);
    // A document without a Project: an applied name, a manual one and scan proposals.
    open_applied(
        &tabs,
        cx,
        dir.path(),
        "loose.md",
        &format!("{text} "),
        &[("Anna Petrova", Category::Person)],
    );
    let loose = active(&tabs, cx);
    detect(
        &loose,
        cx,
        &[
            ("Bob Day", Category::Person),
            ("1 Main St", Category::Address),
        ],
    );
    loose.update(cx, |view, cx| {
        let source = view.editor.read(cx).text().to_owned();
        let start = source.find("Cara Lin").unwrap();
        view.pii
            .review_mut()
            .add_manual(&source, start..start + 8, Category::Person)
            .unwrap();
    });
    let before = originals(&loose, cx);
    assert!(before.contains(&"Bob Day".to_string()), "{before:?}");

    set_default(&tabs, cx, Category::Person, false);
    let after = originals(&loose, cx);
    assert!(!after.contains(&"Bob Day".to_string()), "{after:?}");
    assert!(
        after.contains(&"Cara Lin".to_string()),
        "manual additions stay"
    );
    assert!(
        after.contains(&"1 Main St".to_string()),
        "other categories stay"
    );
    cx.update(|_, cx| {
        let view = loose.read(cx);
        assert!(view.editor.read(cx).text().starts_with("PERSON_1 met"));
        assert_eq!(view.pii.review().applied().len(), 1, "applied stays");
    });
    assert!(
        originals(&member_view, cx).contains(&"Bob Day".to_string()),
        "the document follows its Project"
    );
}

#[gpui::test]
fn toolbar_settings_opens_and_closes_without_editor_focus(cx: &mut TestAppContext) {
    cx.update(crate::settings::bind_keys);
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
        assert_eq!(
            panel.read(cx).category(),
            crate::settings::Category::Detection
        );
    });
    cx.simulate_resize(size(px(640.), px(480.)));
    let dialog = cx.debug_bounds("settings-dialog").unwrap();
    assert!(dialog.size.width <= px(640.));
    assert!(dialog.size.height <= px(480.));
    // The close control stays in the corner at the minimum viewport.
    let bounds = cx.debug_bounds("settings-close").unwrap();
    assert!(bounds.top() >= px(0.) && bounds.bottom() <= px(480.));
    cx.dispatch_action(crate::settings::CloseSettings);
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
fn toolbar_theme_toggle_works_without_editor_focus(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1400.), px(850.)));
    // The sidebar's theme control cycles System → Light → Dark.
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
    cx.update(crate::settings::bind_keys);
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
    cx.dispatch_action(crate::settings::CloseSettings);
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
    cx.update(crate::settings::bind_keys);
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
        cx.dispatch_action(crate::settings::CloseSettings);
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
    cx.update(crate::settings::bind_keys);
    let (tabs, cx) = boot_home(
        cx,
        Session {
            theme_choice: ThemeChoice::Dark,
            ..Session::default()
        },
    );
    cx.simulate_resize(size(px(640.), px(480.)));
    expand_sidebar(&tabs, cx);
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
fn settings_keyboard_traversal_stays_in_dialog_and_restores_opener(cx: &mut TestAppContext) {
    cx.update(crate::settings::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(640.), px(480.)));
    expand_sidebar(&tabs, cx);
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
            assert_eq!(
                panel.read(cx).category(),
                crate::settings::Category::General
            );
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
    cx.update(crate::settings::bind_keys);
    let (tabs, cx) = boot_home(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    click_toolbar(cx, "Settings");
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let category_focus = cx.update(|window, cx| window.focused(cx).unwrap());
    for category in [
        crate::settings::Category::Detection,
        crate::settings::Category::Recognition,
        crate::settings::Category::Spelling,
        crate::settings::Category::Vault,
        crate::settings::Category::AiConnector,
        crate::settings::Category::About,
        crate::settings::Category::General,
    ] {
        key(cx, "down");
        cx.update(|window, cx| {
            assert_eq!(panel.read(cx).category(), category);
            assert_eq!(window.focused(cx), Some(category_focus.clone()));
        });
    }
    key(cx, "up");
    cx.update(|_, cx| assert_eq!(panel.read(cx).category(), crate::settings::Category::About));
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
            settings::Model::Ocr(_) => crate::settings::Category::Recognition,
            settings::Model::Pii(_) => crate::settings::Category::Detection,
        };
        cx.update(|_, cx| {
            assert!(panel.read(cx).open);
            assert_eq!(panel.read(cx).category(), expected, "{model:?}");
        });
    }
    panel.update_in(cx, |panel, window, cx| panel.show_updates(window, cx));
    cx.update(|_, cx| {
        assert_eq!(
            panel.read(cx).category(),
            crate::settings::Category::General
        )
    });
    panel.update_in(cx, |panel, window, cx| {
        panel.show_category(crate::settings::Category::About, window, cx);
        panel.close(window, cx);
        panel.show(window, cx);
    });
    cx.update(|_, cx| {
        assert_eq!(
            panel.read(cx).category(),
            crate::settings::Category::General
        )
    });
}

#[gpui::test]
fn dictionary_view_searches_adds_removes_with_undo_and_stays_virtualized(cx: &mut TestAppContext) {
    cx.update(crate::settings::bind_keys);
    let (tabs, cx) = boot(cx, Session::default());
    cx.simulate_resize(size(px(1100.), px(760.)));
    let panel = cx.update(|_, cx| tabs.read(cx).settings.clone());
    let shared = cx.update(|_, cx| panel.read(cx).shared.clone());
    let words: std::collections::BTreeSet<String> =
        (0..300).map(|i| format!("word{i:03}")).collect();
    cx.update(|_, cx| crate::settings::dictionary::set_words(&shared, words, cx));
    click_toolbar(cx, "Settings");
    click_toolbar(cx, "settings-category-Spelling");
    let scroll = cx.debug_bounds("settings-scroll").unwrap();
    cx.simulate_event(gpui::ScrollEvent {
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
        assert_eq!(
            panel.read(cx).category(),
            crate::settings::Category::Spelling
        );
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
fn pii_download_retains_card_focus(cx: &mut TestAppContext) {
    download_retains_card_focus(cx, settings::Model::Pii(settings::PiiModel::Fp16));
}

/// With automatic checks off, launch makes no update request: only a check
/// the user starts goes online.
#[gpui::test]
fn turning_automatic_checks_off_stops_the_launch_check(cx: &mut TestAppContext) {
    let (tabs, cx) = boot(cx, Session::default());
    let fetches = || crate::app::updater::TEST_FETCHES.load(std::sync::atomic::Ordering::SeqCst);
    for (automatic, sent) in [(false, 0), (true, 1)] {
        tabs.update(cx, |tabs, cx| {
            let mut preferences = tabs.preferences.borrow().snapshot().unwrap_or_default();
            preferences.updates.check_automatically = automatic;
            tabs.preferences.borrow_mut().current = Some(preferences);
            tabs.check_for_updates_at_launch(cx);
        });
        cx.run_until_parked();
        assert_eq!(fetches(), sent);
    }
}
