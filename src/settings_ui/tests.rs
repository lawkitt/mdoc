use super::*;
use gpui::size;
use gpui::{Focusable, TestAppContext};
use mdoc_store::settings::OcrModel;
use mdoc_store::settings::Store;
#[gpui::test]
fn text_recognition_note_matches_the_offered_languages(cx: &mut TestAppContext) {
    let (panel, cx) = cx.add_window_view(|_, cx| {
        Panel::new(Store::in_memory(), Rc::new(Cell::new(Theme::default())), cx)
    });
    cx.simulate_resize(size(px(800.), px(2400.)));
    for model in OcrModel::ALL {
        panel.update(cx, |panel, cx| {
            panel.open = true;
            panel.category = Category::Recognition;
            panel.draft.ocr.model = model;
            panel.pending.fill(Default::default());
            panel.statuses.fill(Status::Missing);
            cx.notify();
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        assert!(
            cx.debug_bounds("Reads scanned pages in English and Russian.")
                .is_some()
        );
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
            Category::General => expected.updates = defaults.updates,
            Category::Detection => expected.pseudonymization = defaults.pseudonymization,
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
