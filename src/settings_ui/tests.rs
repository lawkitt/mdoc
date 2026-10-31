use super::*;
use gpui::size;
use gpui::{Focusable, TestAppContext};
use mdoc_store::settings::OcrModel;
use mdoc_store::settings::Store;

#[gpui::test]
fn existing_panel_reloads_checks_saved_by_another_panel(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let path = dir.path().join("model.onnx");
    std::fs::write(&path, b"verified fixture").unwrap();
    let files = Files::read(std::slice::from_ref(&path));
    let shared = Store::open(Some(settings));
    let first = cx.new(|cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    let second = cx.new(|cx| Panel::new(shared, Rc::new(Cell::new(Theme::Dark)), cx));
    first.update(cx, |p, _| {
        p.model_state
            .record(Model::ALL[0], &files, files.clone(), Status::Ready);
        p.model_state.save().unwrap();
    });
    second.update(cx, |p, cx| {
        p.model_files = Some([vec![path], vec![], vec![], vec![]]);
        p.check_blocked = true;
        p.refresh(cx);
        assert_eq!(*p.row_status(Model::ALL[0]), Status::Ready);
        assert_eq!(p.statuses[0], Status::Unknown);
        assert!(p.ocr_installations.is_empty());
    });
}

#[gpui::test]
fn reopening_settings_and_a_new_panel_show_the_last_check_at_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(
            Store::open(Some(path.clone())),
            Rc::new(Cell::new(Theme::Light)),
            cx,
        )
    });
    panel.update_in(cx, |p, window, cx| {
        p.show(window, cx);
        assert!(p.row_checking(Model::ALL[0]));
    });
    cx.run_until_parked();
    panel.update_in(cx, |p, window, cx| {
        assert!(p.statuses.iter().all(|status| *status == Status::Missing));
        p.close(window, cx);
        p.show_category(Category::Recognition, window, cx);
        assert!(p.statuses.iter().all(|status| *status == Status::Missing));
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.update(|_, cx| assert!(!panel.read(cx).row_checking(Model::ALL[0])));
    panel.update_in(cx, |p, window, cx| p.close(window, cx));
    cx.run_until_parked();
    let shared = Store::open(Some(path));
    let other = cx.new(|cx| Panel::new(shared, Rc::new(Cell::new(Theme::Dark)), cx));
    other.update(cx, |p, cx| {
        p.refresh(cx);
        assert!(p.checking);
        assert!(
            Model::ALL
                .into_iter()
                .all(|model| *p.row_status(model) == Status::Missing)
        );
        assert!(p.statuses.iter().all(|status| *status == Status::Unknown));
    });
    cx.run_until_parked();
}

#[gpui::test]
fn changed_model_files_are_unknown_then_missing_after_the_quiet_check(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let model = Model::ALL[0];
    let path = dir.path().join("model.onnx");
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::Light)), cx)
    });
    for removed in [false, true] {
        std::fs::write(&path, b"model").unwrap();
        let files = Files::read(std::slice::from_ref(&path));
        panel.update(cx, |p, _| {
            p.model_files = Some([vec![path.clone()], vec![], vec![], vec![]]);
            p.model_state
                .record(model, &files, files.clone(), Status::Ready);
        });
        if removed {
            std::fs::remove_file(&path).unwrap();
        } else {
            std::fs::write(&path, b"resized model").unwrap();
        }
        // A running document job must postpone verification while invalidating
        // the old Installed label immediately.
        panel.update(cx, |p, cx| {
            p.check_blocked = true;
            p.refresh(cx);
            assert_eq!(*p.row_status(model), Status::Unknown);
            assert!(p.check_pending);
            assert!(!p.row_checking(model));
        });
        panel.update(cx, |p, cx| {
            p.check_blocked = false;
            p.refresh(cx);
            assert!(p.checking);
            assert_eq!(*p.row_status(model), Status::Unknown);
            assert!(p.row_checking(model));
        });
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(panel.read(cx).statuses[0], Status::Missing));
    }
}

#[gpui::test]
fn remembered_labels_never_supply_runtime_readiness_or_selection_failures(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("model.onnx");
    std::fs::write(&file, b"fixture").unwrap();
    let files = Files::read(std::slice::from_ref(&file));
    let model = Model::Ocr(OcrModel::Cyrillic);
    for status in [
        Status::Ready,
        Status::Missing,
        Status::Damaged("old failure".into()),
        Status::Unavailable("old runtime failure".into()),
    ] {
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::Light)), cx)
        });
        panel.update(cx, |p, cx| {
            p.model_files = Some([vec![], vec![file.clone()], vec![], vec![]]);
            p.model_state
                .record(model, &files, files.clone(), status.clone());
            p.check_blocked = true;
            p.pending.fill(Default::default());
            p.refresh(cx);
            assert_eq!(*p.row_status(model), status);
            assert_eq!(p.statuses[Panel::index(model)], Status::Unknown);
            assert!(p.ocr_installations.is_empty());
            assert!(!p.needs_setup(model));
            p.select(model, cx);
            assert_eq!(p.draft.ocr.model, OcrModel::Cyrillic);
            assert!(p.error.is_none());
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn first_use_setup_and_removal_are_remembered_before_settings_opens(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let paths =
        Model::ALL.map(|model| vec![dir.path().join(format!("{}.model", Panel::index(model)))]);
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(
            Store::open(Some(settings.clone())),
            Rc::new(Cell::new(Theme::Light)),
            cx,
        )
    });
    for model in Model::ALL {
        let index = Panel::index(model);
        std::fs::write(&paths[index][0], b"verified model fixture").unwrap();
        panel.update(cx, |p, cx| {
            p.model_files = Some(paths.clone());
            let completion = SetupCompletion::remember(
                model,
                false,
                &Progress::default(),
                Ok(None),
                p.model_state.clone(),
                Some(Files::read(&paths[index])),
                &paths[index],
            );
            p.finish_setup(model, completion, cx);
            assert_eq!(p.statuses[index], Status::Ready);
        });
        let reopened = cx.new(|cx| {
            Panel::new(
                Store::open(Some(settings.clone())),
                Rc::new(Cell::new(Theme::Dark)),
                cx,
            )
        });
        reopened.update(cx, |p, cx| {
            p.model_files = Some(paths.clone());
            p.check_blocked = true;
            p.refresh(cx);
            assert_eq!(*p.row_status(model), Status::Ready);
            assert!(!p.row_checking(model));
            assert_eq!(p.statuses[index], Status::Unknown);
        });
        std::fs::remove_file(&paths[index][0]).unwrap();
        panel.update(cx, |p, cx| {
            let completion = SetupCompletion::remember(
                model,
                true,
                &Progress::default(),
                Ok(None),
                p.model_state.clone(),
                None,
                &paths[index],
            );
            p.finish_setup(model, completion, cx);
            p.check_blocked = true;
            p.refresh(cx);
            assert_eq!(*p.row_status(model), Status::Missing);
        });
        let stored = ModelState::open(Some(&settings));
        assert_eq!(
            stored.status(model, &Files::read(&paths[index])),
            Some(Status::Missing)
        );
    }
}

#[gpui::test]
fn setup_rejects_files_changed_after_verification_before_publication(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.onnx");
    let settings = dir.path().join("settings.json");
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(
            Store::open(Some(settings.clone())),
            Rc::new(Cell::new(Theme::Light)),
            cx,
        )
    });
    for (model, before_remember) in Model::ALL
        .into_iter()
        .flat_map(|model| [true, false].map(|before_remember| (model, before_remember)))
    {
        std::fs::write(&path, b"verified").unwrap();
        let before = Files::read(std::slice::from_ref(&path));
        // The engine has returned its pre-verification snapshot. An external
        // writer changes a tracked file before the worker publishes success.
        if before_remember {
            std::fs::write(&path, b"changed after verification").unwrap();
        }
        panel.update(cx, |p, cx| {
            p.model_files = Some(Model::ALL.map(|id| {
                if id == model {
                    vec![path.clone()]
                } else {
                    vec![]
                }
            }));
            let completion = SetupCompletion::remember(
                model,
                false,
                &Progress::default(),
                Ok(None),
                p.model_state.clone(),
                Some(before),
                std::slice::from_ref(&path),
            );
            if before_remember {
                assert_eq!(completion.status, Status::Unknown);
                assert!(completion.result.is_err());
            } else {
                assert_eq!(completion.status, Status::Ready);
                // A second gap is the UI handoff after the worker saves.
                std::fs::write(&path, b"changed before UI publication").unwrap();
            }
            p.finish_setup(model, completion, cx);
            assert_eq!(p.statuses[Panel::index(model)], Status::Unknown);
            assert!(p.ocr_installations.is_empty());
            assert!(p.error.is_some());
        });
        assert_eq!(
            ModelState::open(Some(&settings))
                .status(model, &Files::read(std::slice::from_ref(&path))),
            Some(Status::Unknown)
        );
    }
}

#[gpui::test]
fn text_recognition_note_matches_the_offered_languages(cx: &mut TestAppContext) {
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::default())), cx)
    });
    cx.simulate_resize(size(px(800.), px(2400.)));
    for (model, theme, runtime) in OcrModel::ALL.into_iter().flat_map(|model| {
        [Theme::Light, Theme::Dark]
            .into_iter()
            .flat_map(move |theme| [0, 1_000_000].map(|runtime| (model, theme, runtime)))
    }) {
        panel.update(cx, |panel, cx| {
            panel.theme.set(theme);
            panel.open = true;
            panel.category = Category::Recognition;
            panel.draft.ocr.model = model;
            panel.pending.fill(Default::default());
            panel.pending[Panel::index(Model::Ocr(model))].runtime = runtime;
            panel.statuses.fill(Status::Missing);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let note = if runtime == 0 {
            "Default when a document suggests no language. Each document’s card picks its own."
        } else {
            "Default when a document suggests no language. Each document’s card picks its own. The first download adds a 1 MB shared runtime."
        };
        assert!(cx.debug_bounds(note).is_some());
    }
    assert_eq!(ocr::title(OcrModel::V6Small), "English");
    assert_eq!(ocr::title(OcrModel::Cyrillic), "English & Russian");
}

#[gpui::test]
fn all_download_choices_show_source_and_license_without_details(cx: &mut TestAppContext) {
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::default())), cx)
    });
    cx.simulate_resize(size(px(800.), px(2400.)));
    panel.update(cx, |panel, cx| {
        panel.open = true;
        panel.statuses.fill(Status::Missing);
        assert!(panel.details.is_none());
        cx.notify();
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    for model in Model::ALL {
        panel.update_in(cx, |p, window, cx| {
            p.change_category(
                if matches!(model, Model::Ocr(_)) {
                    Category::Recognition
                } else {
                    Category::Detection
                },
                window,
                cx,
            )
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let label = format!("{}: {}", model.name(), models::attribution_label(model));
        assert!(
            cx.debug_bounds(Box::leak(label.clone().into_boxed_str()))
                .is_some(),
            "{label}"
        );
    }
}
#[gpui::test]
fn keyboard_focus_reveals_choices_in_small_scrolling_panel(cx: &mut TestAppContext) {
    cx.update(bind_keys);
    cx.update(ui::bind_keys);
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::default())), cx)
    });
    cx.simulate_resize(size(px(640.), px(480.)));
    panel.update_in(cx, |p, window, cx| {
        p.open = true;
        p.category = Category::Detection;
        p.statuses.fill(Status::Ready);
        window.focus(&p.focus, cx);
        cx.notify();
    });
    let mut reached = false;
    for _ in 0..12 {
        cx.simulate_keystrokes("tab");
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
        }
        reached = cx.update(|window, cx| {
            panel
                .read(cx)
                .controls
                .borrow()
                .get(&gpui::ElementId::from(("select-model", 3usize)))
                .is_some_and(|focus| focus.is_focused(window))
        });
        if reached {
            break;
        }
    }
    assert!(reached, "Tab must visit the last model choice");
    let choice = cx.debug_bounds("settings-select-fp32").unwrap();
    let body = cx.debug_bounds("settings-scroll").unwrap();
    assert!(
        choice.top() >= body.top() && choice.bottom() <= body.bottom(),
        "choice={choice:?}, body={body:?}"
    );
}

#[gpui::test]
fn tab_list_keys_show_categories_and_tab_enters_content(cx: &mut TestAppContext) {
    cx.update(bind_keys);
    cx.update(ui::bind_keys);
    for theme in [Theme::Light, Theme::Dark] {
        let (panel, cx) = cx
            .add_window_view(|_, cx| Panel::new(Store::in_memory(), Rc::new(Cell::new(theme)), cx));
        cx.simulate_resize(size(px(620.), px(480.)));
        panel.update_in(cx, |p, w, cx| {
            p.open = true;
            p.statuses.fill(Status::Ready);
            w.focus(&p.category_focus, cx);
            cx.notify();
        });
        for category in Category::ALL.into_iter().skip(1).chain([Category::General]) {
            cx.simulate_keystrokes("down");
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
                assert_eq!(panel.read(cx).category(), category);
                assert!(panel.read(cx).category_focus.is_focused(w));
            });
        }
        cx.simulate_keystrokes("up");
        cx.update(|_, cx| assert_eq!(panel.read(cx).category(), Category::About));
        cx.simulate_keystrokes("down tab");
        cx.update(|w, cx| {
            assert!(
                panel
                    .read(cx)
                    .controls
                    .borrow()
                    .get(&gpui::ElementId::from("settings-theme-system"))
                    .unwrap()
                    .is_focused(w)
            )
        });
        // Every category's last control remains reachable and its entire
        // ring stays in the viewport after scrolling in either direction.
        for category in Category::ALL.into_iter().filter(|c| *c != Category::About) {
            panel.update_in(cx, |p, w, cx| p.change_category(category, w, cx));
            let mut reached = false;
            for _ in 0..45 {
                cx.simulate_keystrokes("tab");
                for _ in 0..3 {
                    cx.update(|w, cx| {
                        w.refresh();
                        w.draw(cx).clear(cx);
                    });
                }
                reached = cx.update(|w, cx| {
                    panel
                        .read(cx)
                        .controls
                        .borrow()
                        .get(&gpui::ElementId::from("reset-settings"))
                        .is_some_and(|f| f.is_focused(w))
                });
                if reached {
                    break;
                }
            }
            assert!(reached, "Reset reachable in {category:?}/{theme:?}");
            let ring = cx.debug_bounds("reset-settings").unwrap().dilate(px(4.));
            let viewport = cx.debug_bounds("settings-scroll").unwrap();
            assert!(
                ring.top() >= viewport.top() && ring.bottom() <= viewport.bottom(),
                "{category:?}: {ring:?} in {viewport:?}"
            );
        }
    }
}

/// Settings › General › "Show mdoc in the menu bar" (ADR 0084): macOS
/// only, on by default, saved on change.
#[gpui::test]
fn the_menu_bar_switch_is_on_by_default_and_saves_at_once(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
    });
    cx.simulate_resize(size(px(800.), px(2400.)));
    panel.update(cx, |panel, cx| {
        panel.open = true;
        cx.notify();
    });
    cx.run_until_parked();
    let switch = cx.debug_bounds("general-menu-bar");
    if !cfg!(target_os = "macos") {
        assert!(switch.is_none(), "no tray outside macOS yet");
        return;
    }
    assert!(shared.borrow().snapshot().unwrap().general.menu_bar);
    cx.simulate_click(switch.unwrap().center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(!shared.borrow().snapshot().unwrap().general.menu_bar);
}

#[gpui::test]
fn update_choices_save_immediately_and_an_update_offers_its_release(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
    });
    // Tall enough that the Updates section needs no scrolling.
    cx.simulate_resize(size(px(800.), px(2400.)));
    // Not shown: opening would check models through the global job slot.
    panel.update(cx, |panel, cx| {
        panel.open = true;
        cx.notify();
    });
    cx.run_until_parked();
    let click = |cx: &mut gpui::VisualTestContext, id: &'static str| {
        let bounds = cx.debug_bounds(id).unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
    };
    assert!(cx.debug_bounds("updates-view").is_none());

    click(cx, "updates-automatic");
    let saved = shared.borrow().snapshot().unwrap().updates;
    assert!(!saved.check_automatically);
    click(cx, "updates-prereleases");
    let saved = shared.borrow().snapshot().unwrap().updates;
    // A pre-release build always includes pre-releases; the switch is inert.
    assert_eq!(saved.include_prereleases, !updater::is_prerelease_build());
    // Turning the check back on starts one; tests never reach the network.
    click(cx, "updates-automatic");
    assert!(
        shared
            .borrow()
            .snapshot()
            .unwrap()
            .updates
            .check_automatically
    );
    cx.update(|_, cx| assert!(!updater::state(cx).checking));

    let set = |cx: &mut gpui::VisualTestContext, state: updater::UpdateState| {
        cx.update(|_, cx| {
            cx.set_global(state);
            cx.refresh_windows();
        });
        cx.run_until_parked();
    };
    let found = updater::UpdateState {
        checked: true,
        available: Some(updater::test_update("9.0.0")),
        ..Default::default()
    };
    // Until this copy is known to be eligible, only the release page.
    set(cx, found.clone());
    assert!(cx.debug_bounds("updates-view").is_some());
    assert!(cx.debug_bounds("updates-install").is_none());
    // An eligible copy offers the install; a download can be cancelled.
    set(
        cx,
        updater::UpdateState {
            eligibility: Some(Ok(())),
            ..found.clone()
        },
    );
    assert!(cx.debug_bounds("updates-install").is_some());
    set(
        cx,
        updater::UpdateState {
            eligibility: Some(Ok(())),
            install: updater::Install::Downloading {
                version: "9.0.0".into(),
                received: 10,
                total: 100,
                progress: updater::Progress::default(),
            },
            ..found.clone()
        },
    );
    assert!(cx.debug_bounds("updates-install").is_none());
    click(cx, "updates-cancel");
    cx.update(|_, cx| assert!(matches!(updater::state(cx).install, updater::Install::Idle)));
    // A staged update offers Restart now, which the workspace carries out.
    set(
        cx,
        updater::UpdateState {
            eligibility: Some(Ok(())),
            install: updater::Install::Ready {
                version: "9.0.0".into(),
                staged: Default::default(),
            },
            ..found
        },
    );
    let restarted = Rc::new(Cell::new(false));
    let seen = restarted.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&panel, move |_, event, _| {
            if matches!(event, Event::RestartToUpdate) {
                seen.set(true);
            }
        })
    });
    click(cx, "updates-restart");
    assert!(restarted.get());
}
#[gpui::test]
fn numbers_commit_on_enter_and_blur_and_reject_invalid_values(cx: &mut TestAppContext) {
    cx.update(bind_keys);
    cx.update(ui::bind_keys);
    cx.update(crate::markdown_search::bind_keys);
    let shared = Store::in_memory();
    let (panel, cx) = cx
        .add_window_view(|_, cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    cx.simulate_resize(size(px(1000.), px(800.)));
    cx.update(|w, _| w.activate_window());
    panel.update_in(cx, |p, w, cx| {
        p.open = true;
        p.change_category(Category::Detection, w, cx);
    });
    cx.run_until_parked();
    for bad in ["no", "NaN", "inf", "-0.1", "1.1", ""] {
        panel.update_in(cx, |p, w, cx| {
            p.threshold.update(cx, |i, cx| i.set_value(bad.into(), cx));
            w.focus(&p.threshold.read(cx).focus_handle(cx), cx);
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            shared
                .borrow()
                .snapshot()
                .unwrap()
                .pseudonymization
                .threshold,
            0.5
        );
        panel.update(cx, |p, _| {
            assert!(p.number_errors[0].is_some());
            assert_eq!(p.draft.pseudonymization.threshold, 0.5);
        });
    }
    panel.update(cx, |p, cx| {
        p.threshold
            .update(cx, |i, cx| i.set_value("0.3".into(), cx))
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        shared
            .borrow()
            .snapshot()
            .unwrap()
            .pseudonymization
            .threshold,
        0.3
    );
    panel.update_in(cx, |p, w, cx| {
        p.change_category(Category::Recognition, w, cx)
    });
    cx.run_until_parked();
    panel.update_in(cx, |p, w, cx| {
        p.confidence
            .update(cx, |i, cx| i.set_value("0.7".into(), cx));
        w.focus(&p.confidence.read(cx).focus_handle(cx), cx);
    });
    cx.run_until_parked();
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
        assert!(
            panel
                .read(cx)
                .confidence
                .read(cx)
                .focus_handle(cx)
                .is_focused(w)
        );
    });
    // Tab blurs the field and persists without a Save button.
    cx.simulate_keystrokes("tab");
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
        assert!(
            !panel
                .read(cx)
                .confidence
                .read(cx)
                .focus_handle(cx)
                .is_focused(w)
        );
    });
    cx.run_until_parked();
    assert_eq!(
        shared.borrow().snapshot().unwrap().ocr.minimum_confidence,
        0.7
    );
    assert_eq!(
        shared
            .borrow()
            .snapshot()
            .unwrap()
            .pseudonymization
            .threshold,
        0.3
    );
}

#[gpui::test]
fn reset_is_per_category_and_preserves_dictionary(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    let words = std::sync::Arc::new(["Lawkitt".to_string()].into_iter().collect());
    shared.borrow_mut().words.words = std::sync::Arc::clone(&words);
    let mut custom = Preferences::default();
    custom.ocr.model = OcrModel::Cyrillic;
    custom.ocr.dpi = 300;
    custom.ocr.minimum_confidence = 0.6;
    custom.pseudonymization.model = PiiModel::Fp32;
    custom.pseudonymization.threshold = 0.8;
    custom.spelling.enabled = false;
    custom.updates.check_automatically = false;
    custom.cases.ask_before_closing = false;
    custom.categories.set(mdoc_pii::Category::Person, false);
    custom.general.menu_bar = false;
    // Notes already seen stay seen through a reset.
    custom.general.menu_bar_note_shown = true;
    custom.general.shelf_drag_hint_shown = true;
    for category in Category::ALL {
        shared.borrow_mut().current = Some(custom.clone());
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx)
        });
        panel.update(cx, |p, cx| {
            p.category = category;
            p.reset(cx);
        });
        cx.run_until_parked();
        let mut expected = custom.clone();
        let defaults = Preferences::default();
        match category {
            Category::General => {
                expected.updates = defaults.updates;
                expected.general.menu_bar = defaults.general.menu_bar;
            }
            Category::Detection => {
                expected.pseudonymization = defaults.pseudonymization;
                // Reset ticks every category again (ADR 0076).
                expected.categories = defaults.categories;
            }
            Category::Recognition => expected.ocr = defaults.ocr,
            Category::Spelling => expected.spelling = defaults.spelling,
            Category::Cases => expected.cases = defaults.cases,
            Category::About => {}
        }
        assert_eq!(
            shared.borrow().snapshot().unwrap(),
            expected,
            "{category:?}"
        );
        assert_eq!(shared.borrow().words.words, words);
    }
}

#[gpui::test]
fn model_choices_save_immediately_and_failures_revert(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
    });
    // Not shown: opening would check models through the global job slot.
    panel.update(cx, |panel, cx| panel.select(Model::Pii(PiiModel::Fp32), cx));
    cx.run_until_parked();
    let current = shared.borrow().snapshot().unwrap();
    assert_eq!(current.pseudonymization.model, PiiModel::Fp32);
    // A selected missing model downloads on first use; nothing starts now.
    panel.update(cx, |panel, _| {
        assert!(panel.working.is_none());
        assert!(panel.needs_setup(Model::Pii(PiiModel::Fp32)));
    });

    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("not-a-directory");
    std::fs::write(&invalid, b"blocked").unwrap();
    shared.borrow_mut().path = Some(invalid.join("settings.json"));
    panel.update(cx, |panel, cx| {
        panel.select(Model::Ocr(OcrModel::Cyrillic), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        shared.borrow().snapshot().unwrap().ocr.model,
        OcrModel::V6Small
    );
    panel.update(cx, |panel, _| {
        assert_eq!(panel.draft.ocr.model, OcrModel::V6Small);
        assert!(panel.error.as_ref().unwrap().contains("Previous defaults"));
    });

    shared.borrow_mut().path = None;
    panel.update(cx, |panel, cx| {
        panel.category = Category::Detection;
        panel.reset(cx);
    });
    cx.run_until_parked();
    assert_eq!(shared.borrow().snapshot().unwrap(), Preferences::default());
}
#[gpui::test]
fn failed_save_restores_numbers_cases_and_models(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("not-a-directory");
    std::fs::write(&invalid, b"blocked").unwrap();
    shared.borrow_mut().path = Some(invalid.join("settings.json"));
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
    });
    panel.update(cx, |panel, cx| {
        panel.draft.ocr.dpi = 300;
        panel.draft.ocr.minimum_confidence = 0.9;
        panel.draft.pseudonymization.threshold = 0.2;
        panel.draft.cases.ask_before_closing = false;
        panel.persist(cx);
    });
    cx.run_until_parked();
    assert_eq!(shared.borrow().snapshot().unwrap().ocr.dpi, 150);
    panel.update(cx, |panel, _| {
        assert_eq!(panel.draft, Preferences::default());
        assert!(panel.error.as_ref().unwrap().contains("Previous defaults"));
    });
    panel.update(cx, |panel, cx| {
        panel
            .threshold
            .update(cx, |i, cx| i.set_value("NaN".into(), cx));
        panel.commit_number(0, cx);
    });
    panel.update(cx, |p, _| assert!(p.number_errors[0].is_some()));
    assert_eq!(
        shared
            .borrow()
            .snapshot()
            .unwrap()
            .pseudonymization
            .threshold,
        0.5
    );
}

#[gpui::test]
fn a_number_blur_during_a_save_is_persisted_after_it(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    let (panel, cx) = cx
        .add_window_view(|_, cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    panel.update(cx, |p, cx| {
        p.select(Model::Pii(PiiModel::Fp32), cx);
        assert!(p.applying);
        p.threshold
            .update(cx, |i, cx| i.set_value("0.7".into(), cx));
        p.commit_number(0, cx);
        assert!(p.save_pending);
    });
    cx.run_until_parked();
    assert_eq!(
        shared
            .borrow()
            .snapshot()
            .unwrap()
            .pseudonymization
            .threshold,
        0.7
    );
    assert_eq!(
        shared.borrow().snapshot().unwrap().pseudonymization.model,
        PiiModel::Fp32
    );
}

#[gpui::test]
fn general_reset_changes_theme_only_after_save_and_respects_a_newer_choice(
    cx: &mut TestAppContext,
) {
    let shared = Store::in_memory();
    let (panel, cx) = cx
        .add_window_view(|_, cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let events = seen.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&panel, move |_, event, _| {
            if let Event::ThemeSelected(choice) = event {
                events.borrow_mut().push(*choice);
            }
        })
    });
    panel.update(cx, |p, cx| {
        p.sync_theme(ThemeChoice::Dark, cx);
        p.reset(cx);
        assert!(seen.borrow().is_empty());
        p.sync_theme(ThemeChoice::Light, cx);
    });
    cx.run_until_parked();
    assert!(seen.borrow().is_empty(), "a later theme choice wins");
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("not-a-directory");
    std::fs::write(&invalid, b"blocked").unwrap();
    shared.borrow_mut().path = Some(invalid.join("settings.json"));
    panel.update(cx, |p, cx| p.reset(cx));
    cx.run_until_parked();
    assert!(
        seen.borrow().is_empty(),
        "a failed reset must keep the theme"
    );
    shared.borrow_mut().path = None;
    panel.update(cx, |p, cx| p.reset(cx));
    cx.run_until_parked();
    assert_eq!(*seen.borrow(), vec![ThemeChoice::System]);
}

#[gpui::test]
fn resetting_another_category_preserves_number_text_and_its_error(cx: &mut TestAppContext) {
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::Light)), cx)
    });
    panel.update(cx, |p, cx| {
        p.threshold
            .update(cx, |i, cx| i.set_value("invalid".into(), cx));
        p.commit_number(0, cx);
        p.category = Category::Spelling;
        p.reset(cx);
    });
    cx.run_until_parked();
    panel.update(cx, |p, cx| {
        assert_eq!(p.threshold.read(cx).value(), "invalid");
        assert!(p.number_errors[0].is_some());
        p.category = Category::Detection;
        p.reset(cx);
    });
    cx.run_until_parked();
    panel.update(cx, |p, cx| {
        assert_eq!(p.threshold.read(cx).value(), "0.5");
        assert!(p.number_errors[0].is_none());
    });
}

/// Review of #280: a shelf flag saved while a panel save is in flight
/// survives that save's completion, and keeps the choices it was saving,
/// the menu bar switch among them.
#[gpui::test]
fn a_shelf_flag_saved_during_a_panel_save_survives_it(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    let (panel, cx) = cx
        .add_window_view(|_, cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    // A General reset is among the choices saving: it resets the theme.
    let reset = Rc::new(Cell::new(false));
    cx.update(|_, cx| {
        let reset = reset.clone();
        cx.subscribe(&panel, move |_, event: &Event, _| {
            if matches!(event, Event::ThemeSelected(ThemeChoice::System)) {
                reset.set(true);
            }
        })
        .detach()
    });
    panel.update(cx, |p, cx| {
        hold_next_save(p, cx);
        p.draft.updates.check_automatically = false;
        p.draft.general.menu_bar = false;
        p.reset_theme = Some(p.theme_revision);
        p.persist(cx);
    });
    // The shelf records its note while that save waits, from preferences
    // that still have the switch on; as `Workspace::update_general`, it is
    // a later write.
    let note =
        |general: &mut mdoc_store::settings::GeneralConfig| general.menu_bar_note_shown = true;
    mdoc_store::settings::next_write();
    note(&mut shared.borrow_mut().current.as_mut().unwrap().general);
    panel.update(cx, |p, cx| p.general_changed(note, cx));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    let saved = shared.borrow().snapshot().unwrap();
    assert!(saved.general.menu_bar_note_shown);
    assert!(!saved.general.menu_bar, "the switch-off is not undone");
    assert!(!saved.updates.check_automatically);
    assert!(reset.get(), "the reset's theme change is not lost");
}

/// Review of #280: a Settings save that a shelf note's save overtook
/// publishes nothing, so even when the save after it fails, the older
/// choice (the menu bar switch on) never comes back.
#[gpui::test]
fn an_overtaken_settings_save_never_brings_an_older_choice_back(cx: &mut TestAppContext) {
    use mdoc_store::settings::{GeneralConfig, load, next_write, save_in_order};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let shared = Store::in_memory();
    shared.borrow_mut().path = Some(path.clone());
    let (panel, cx) = cx
        .add_window_view(|_, cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    panel.update(cx, |p, cx| {
        hold_next_save(p, cx);
        p.draft.updates.check_automatically = false;
        p.persist(cx);
    });
    // The note's Quit when windows close, as `Workspace::update_general`.
    let off = |general: &mut GeneralConfig| general.menu_bar = false;
    let mut changed = shared.borrow().current.clone().unwrap();
    off(&mut changed.general);
    assert!(save_in_order(&path, &changed, next_write()).unwrap());
    shared.borrow_mut().current = Some(changed);
    panel.update(cx, |p, cx| p.general_changed(off, cx));
    // The save after the overtaken one fails.
    let blocked = dir.path().join("not-a-directory");
    std::fs::write(&blocked, b"blocked").unwrap();
    shared.borrow_mut().path = Some(blocked.join("settings.json"));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(!shared.borrow().snapshot().unwrap().general.menu_bar);
    panel.update(cx, |p, _| assert!(!p.draft.general.menu_bar));
    assert!(!load(&path).unwrap().general.menu_bar, "on disk too");
}

fn hold_next_save(panel: &mut Panel, cx: &mut Context<Panel>) {
    let gate = cx.background_executor().timer(Duration::from_secs(1));
    panel.save_gate = Some(cx.background_executor().spawn(gate));
}

#[gpui::test]
fn a_failed_number_save_after_close_is_visible_on_reopening(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("not-a-directory");
    std::fs::write(&invalid, b"blocked").unwrap();
    shared.borrow_mut().path = Some(invalid.join("settings.json"));
    let (panel, cx) = cx
        .add_window_view(|_, cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    panel.update_in(cx, |p, w, cx| {
        p.open = true;
        p.category = Category::Detection;
        p.threshold
            .update(cx, |i, cx| i.set_value("0.7".into(), cx));
        p.close(w, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |p, w, cx| p.show_category(Category::Detection, w, cx));
    cx.update(|w, cx| {
        w.refresh();
        w.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("settings-error").is_some());
    panel.update(cx, |p, cx| {
        assert_eq!(p.threshold.read(cx).value(), "0.5");
        assert!(
            p.error
                .as_ref()
                .unwrap()
                .contains("Could not save settings")
        );
        assert!(p.error.as_ref().unwrap().contains("Previous defaults"));
    });
    assert_eq!(
        shared
            .borrow()
            .snapshot()
            .unwrap()
            .pseudonymization
            .threshold,
        0.5
    );
}

#[gpui::test]
fn blur_autosave_keeps_the_mouse_click_on_resolution_model_and_reset(cx: &mut TestAppContext) {
    cx.update(bind_keys);
    cx.update(ui::bind_keys);
    for target in ["settings-dpi-300", "settings-select-fp32", "reset-settings"] {
        let shared = Store::in_memory();
        let (panel, cx) = cx.add_window_view(|_, cx| {
            Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx)
        });
        cx.simulate_resize(size(px(1100.), px(800.)));
        cx.update(|w, _| w.activate_window());
        let recognition = target == "settings-dpi-300";
        panel.update_in(cx, |p, w, cx| {
            p.open = true;
            p.statuses.fill(Status::Ready);
            p.change_category(
                if recognition {
                    Category::Recognition
                } else {
                    Category::Detection
                },
                w,
                cx,
            );
        });
        cx.run_until_parked();
        panel.update_in(cx, |p, w, cx| {
            let input = if recognition {
                &p.confidence
            } else {
                &p.threshold
            };
            input.update(cx, |i, cx| i.set_value("0.7".into(), cx));
            w.focus(&input.read(cx).focus_handle(cx), cx);
            hold_next_save(p, cx);
        });
        for _ in 0..3 {
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            });
        }
        if target == "reset-settings" {
            // Detection is taller than the overlay; bring Reset into view.
            panel.update(cx, |p, _| {
                let bottom = p.scroll.max_offset().y;
                p.scroll.set_offset(gpui::point(px(0.), -bottom));
            });
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear(cx);
            });
        }
        let position = cx.debug_bounds(target).unwrap().center();
        cx.simulate_event(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        panel.update(cx, |p, _| {
            assert!(p.applying, "{target}: blur must start the held save")
        });
        cx.simulate_event(gpui::MouseUpEvent {
            position,
            button: gpui::MouseButton::Left,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
        });
        cx.run_until_parked();
        panel.update(cx, |p, _| {
            assert!(
                p.save_pending,
                "{target}: mouse-up must queue the second change"
            )
        });
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        let saved = shared.borrow().snapshot().unwrap();
        match target {
            "settings-dpi-300" => {
                assert_eq!(saved.ocr.dpi, 300);
                assert_eq!(saved.ocr.minimum_confidence, 0.7);
            }
            "settings-select-fp32" => {
                assert_eq!(saved.pseudonymization.model, PiiModel::Fp32);
                assert_eq!(saved.pseudonymization.threshold, 0.7);
            }
            _ => assert_eq!(
                saved.pseudonymization,
                Preferences::default().pseudonymization
            ),
        }
    }
}

#[gpui::test]
fn a_queued_general_reset_changes_theme_only_if_its_own_save_succeeds(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    shared
        .borrow_mut()
        .current
        .as_mut()
        .unwrap()
        .updates
        .check_automatically = false;
    let (panel, cx) = cx
        .add_window_view(|_, cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    let seen = Rc::new(Cell::new(false));
    let events = seen.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&panel, move |_, event, _| {
            if matches!(event, Event::ThemeSelected(_)) {
                events.set(true);
            }
        })
    });
    panel.update(cx, |p, cx| {
        hold_next_save(p, cx);
        p.select(Model::Pii(PiiModel::Fp32), cx);
        p.reset(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("not-a-directory");
    std::fs::write(&invalid, b"blocked").unwrap();
    shared.borrow_mut().path = Some(invalid.join("settings.json"));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(!seen.get());
    assert_eq!(
        shared.borrow().snapshot().unwrap().pseudonymization.model,
        PiiModel::Fp32
    );
    assert!(
        !shared
            .borrow()
            .snapshot()
            .unwrap()
            .updates
            .check_automatically
    );
}

#[gpui::test]
fn detection_categories_save_on_change_by_mouse_and_keyboard(cx: &mut TestAppContext) {
    cx.update(bind_keys);
    cx.update(ui::bind_keys);
    let shared = Store::in_memory();
    let (panel, cx) = cx
        .add_window_view(|_, cx| Panel::new(shared.clone(), Rc::new(Cell::new(Theme::Light)), cx));
    cx.simulate_resize(size(px(800.), px(2400.)));
    let unticked = Rc::new(std::cell::RefCell::new(Vec::new()));
    cx.update(|_, cx| {
        let unticked = unticked.clone();
        cx.subscribe(&panel, move |_, event: &Event, _| {
            if let Event::CategoriesUnticked(categories) = event {
                unticked.borrow_mut().push(categories.clone());
            }
        })
        .detach();
    });
    // Not shown: opening would check models through the global job slot.
    panel.update(cx, |p, cx| {
        p.open = true;
        p.category = Category::Detection;
        p.statuses.fill(Status::Ready);
        cx.notify();
    });
    cx.run_until_parked();
    let click = |cx: &mut gpui::VisualTestContext, id: &'static str| {
        let bounds = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("missing {id}"));
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
    };
    let skip =
        |shared: &mdoc_store::settings::Shared| shared.borrow().snapshot().unwrap().categories.skip;
    // Every category scans return is listed and on; Date and Other are manual only.
    for category in mdoc_pii::Category::DETECTED {
        let id: &'static str = format!("settings-detect-{}", category.token()).leak();
        assert!(cx.debug_bounds(id).is_some(), "{id}");
    }
    assert!(cx.debug_bounds("settings-detect-REDACTED").is_none());
    assert!(cx.debug_bounds("settings-detect-DATE").is_none());
    assert!(skip(&shared).is_empty());

    click(cx, "settings-detect-ADDRESS");
    click(cx, "settings-detect-EMAIL");
    assert_eq!(skip(&shared), ["ADDRESS", "EMAIL"]);
    assert_eq!(
        *unticked.borrow(),
        [
            [mdoc_pii::Category::Address].into(),
            [mdoc_pii::Category::Email].into()
        ]
    );
    // Ticking again saves too, and unticks nothing.
    click(cx, "settings-detect-EMAIL");
    assert_eq!(skip(&shared), ["ADDRESS"]);
    assert_eq!(unticked.borrow().len(), 2);

    // Keyboard: Tab reaches the Person chip and Space unticks it.
    let person = gpui::ElementId::from(gpui::SharedString::from("settings-detect-PERSON"));
    let focused = |cx: &mut gpui::VisualTestContext| {
        cx.update(|w, cx| {
            panel
                .read(cx)
                .controls
                .borrow()
                .get(&person)
                .is_some_and(|f| f.is_focused(w))
        })
    };
    panel.update_in(cx, |p, w, cx| w.focus(&p.category_focus, cx));
    let mut reached = false;
    for _ in 0..30 {
        cx.simulate_keystrokes("tab");
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
        if focused(cx) {
            reached = true;
            break;
        }
    }
    assert!(reached, "the Person chip is reachable by Tab");
    cx.simulate_keystrokes("space");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("space").unwrap(),
    });
    cx.run_until_parked();
    assert_eq!(skip(&shared), ["ADDRESS", "PERSON"]);

    // Reset ticks all again.
    panel.update(cx, |p, cx| p.reset(cx));
    cx.run_until_parked();
    assert!(skip(&shared).is_empty());
    assert!(panel.read_with(cx, |p, _| p.draft.categories.skip.is_empty()));
}

#[gpui::test]
fn a_failed_category_save_restores_the_previous_list_and_says_so(cx: &mut TestAppContext) {
    let shared = Store::in_memory();
    let mut stored = Preferences::default();
    stored.categories.set(mdoc_pii::Category::Phone, false);
    shared.borrow_mut().current = Some(stored);
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(shared.clone(), Rc::new(Cell::new(Theme::default())), cx)
    });
    let unticked = Rc::new(Cell::new(false));
    cx.update(|_, cx| {
        let unticked = unticked.clone();
        cx.subscribe(&panel, move |_, event: &Event, _| {
            if matches!(event, Event::CategoriesUnticked(_)) {
                unticked.set(true);
            }
        })
        .detach();
    });
    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("not-a-directory");
    std::fs::write(&invalid, b"blocked").unwrap();
    shared.borrow_mut().path = Some(invalid.join("settings.json"));
    panel.update(cx, |panel, cx| {
        panel
            .draft
            .categories
            .set(mdoc_pii::Category::Person, false);
        panel.draft.categories.set(mdoc_pii::Category::Phone, true);
        panel.persist(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        shared.borrow().snapshot().unwrap().categories.skip,
        ["PHONE"]
    );
    panel.update(cx, |panel, _| {
        assert_eq!(panel.draft.categories.skip, ["PHONE"]);
        assert!(
            panel
                .error
                .as_ref()
                .unwrap()
                .contains("Could not save settings")
        );
    });
    assert!(
        !unticked.get(),
        "nothing is withdrawn unless the save succeeds"
    );
}

fn draw_settings(cx: &mut gpui::VisualTestContext) {
    for _ in 0..3 {
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear(cx);
        });
    }
}

/// ADR 0076: every switch shares the right-hand column, About has no Reset,
/// and Remove… is offered only while it can run.
#[gpui::test]
fn settings_states_are_told_apart(cx: &mut TestAppContext) {
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::Light)), cx)
    });
    cx.simulate_resize(size(px(1100.), px(2400.)));
    // Not shown: opening would check models through the global job slot.
    panel.update(cx, |p, cx| {
        p.open = true;
        cx.notify();
    });
    draw_settings(cx);
    let right =
        |cx: &mut gpui::VisualTestContext, id: &'static str| cx.debug_bounds(id).unwrap().right();
    let column = right(cx, "updates-automatic");
    assert_eq!(right(cx, "updates-prereleases"), column);
    if cfg!(target_os = "macos") {
        assert_eq!(right(cx, "general-menu-bar"), column);
    }
    panel.update_in(cx, |p, w, cx| p.change_category(Category::Spelling, w, cx));
    draw_settings(cx);
    assert_eq!(right(cx, "spelling-enabled"), column);

    panel.update_in(cx, |p, w, cx| p.change_category(Category::About, w, cx));
    draw_settings(cx);
    assert!(cx.debug_bounds("reset-settings").is_none());

    let (fp16, fp32) = (
        Panel::index(Model::Pii(PiiModel::Fp16)),
        Panel::index(Model::Pii(PiiModel::Fp32)),
    );
    let remove = |cx: &mut gpui::VisualTestContext, index: usize| {
        let ids = [
            "settings-remove-0",
            "settings-remove-1",
            "settings-remove-2",
            "settings-remove-3",
        ];
        cx.debug_bounds(ids[index]).is_some()
    };
    panel.update_in(cx, |p, w, cx| {
        p.statuses.fill(Status::Ready);
        p.change_category(Category::Detection, w, cx);
    });
    draw_settings(cx);
    // The active model and an inactive one alike.
    assert!(remove(cx, fp16) && remove(cx, fp32));
    // Another model job holds the slot: no dimmed Remove… beside Details.
    panel.update(cx, |p, cx| {
        p.working = Some(Model::Ocr(OcrModel::Cyrillic));
        cx.notify();
    });
    draw_settings(cx);
    assert!(!remove(cx, fp16) && !remove(cx, fp32));
    assert!(cx.debug_bounds("settings-details-fp16").is_some());
    // An unconfirmed install offers no Remove… either.
    panel.update(cx, |p, cx| {
        p.working = None;
        p.statuses[fp32] = Status::Unknown;
        cx.notify();
    });
    draw_settings(cx);
    assert!(remove(cx, fp16) && !remove(cx, fp32));
}

/// ADR 0076: a page taller than the overlay fades where rows continue and
/// shows a scroll thumb.
#[gpui::test]
fn a_long_page_shows_where_it_continues(cx: &mut TestAppContext) {
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::Dark)), cx)
    });
    cx.simulate_resize(size(px(1100.), px(560.)));
    panel.update_in(cx, |p, w, cx| {
        p.open = true;
        p.change_category(Category::Detection, w, cx);
    });
    draw_settings(cx);
    assert!(cx.debug_bounds("settings-scrollbar").is_some());
    assert!(cx.debug_bounds("settings-fade-bottom").is_some());
    assert!(cx.debug_bounds("settings-fade-top").is_none());
    panel.update(cx, |p, cx| {
        let end = p.scroll.max_offset().y;
        p.scroll.set_offset(gpui::point(px(0.), -end));
        cx.notify();
    });
    draw_settings(cx);
    assert!(cx.debug_bounds("settings-fade-top").is_some());
    assert!(cx.debug_bounds("settings-fade-bottom").is_none());
    // A page that fits shows neither.
    panel.update_in(cx, |p, w, cx| p.change_category(Category::Cases, w, cx));
    draw_settings(cx);
    assert!(cx.debug_bounds("settings-scrollbar").is_none());
    assert!(cx.debug_bounds("settings-fade-top").is_none());
    assert!(cx.debug_bounds("settings-fade-bottom").is_none());
}
