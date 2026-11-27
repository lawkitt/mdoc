//! Settings › AI connector's switch reaches the window: turned off, the
//! connector stops sharing and stops listening at once, without a
//! relaunch, and it can listen again right away.
use super::*;
use mdoc_mcp::ipc::{self, Endpoint};

/// The window's preview setting changes, as a Settings save publishes it.
fn set_preview(tabs: &Entity<Workspace>, on: bool, cx: &mut VisualTestContext) {
    tabs.update(cx, |tabs, cx| {
        let mut preferences = tabs
            .preferences
            .borrow()
            .current
            .clone()
            .unwrap_or_default();
        preferences.ai_connector.preview = on;
        tabs.preferences.borrow_mut().current = Some(preferences);
        tabs.settings
            .update(cx, |_, cx| cx.emit(settings_ui::Event::Applied));
    });
    cx.run_until_parked();
}

fn listening(endpoint: &Endpoint) -> bool {
    !matches!(
        ipc::call(endpoint, b"{}"),
        Err(ipc::CallError::NotListening)
    )
}

/// The connector on a test endpoint, as launch starts it with the setting
/// on: tests never take the profile's endpoint.
fn start(tabs: &Entity<Workspace>, endpoint: &Endpoint, cx: &mut VisualTestContext) {
    tabs.update(cx, |tabs, _| {
        let vault = tabs.vault.vault().cloned().expect("the test Vault");
        let on = mdoc_store::settings::AiConnectorConfig {
            preview: true,
            ..Default::default()
        };
        tabs.ai_connector = crate::ai_connector::start(vault, &on, Some(endpoint)).unwrap();
        assert!(tabs.ai_connector.is_some(), "listening at once");
    });
}

/// #1220: the switch shown off stops the connector before its save lands;
/// a save that failed starts it again.
#[gpui::test]
fn off_stops_before_its_save_lands(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
    set_preview(&tabs, true, cx);
    start(&tabs, &endpoint, cx);
    assert!(listening(&endpoint));
    // Still saved on.
    tabs.update(cx, |tabs, cx| {
        tabs.settings.update(cx, |_, cx| {
            cx.emit(settings_ui::Event::AiConnectorShown(false))
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).ai_connector.is_none()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while listening(&endpoint) {
        assert!(std::time::Instant::now() < deadline, "still listening");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    tabs.update(cx, |tabs, cx| {
        tabs.settings.update(cx, |_, cx| {
            cx.emit(settings_ui::Event::AiConnectorShown(true))
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(tabs.read(cx).ai_connector.is_some()));
}

/// #1220: a start that fails, here on a name another mdoc holds, is shown
/// in Settings › AI connector, and goes with the switch off.
#[gpui::test]
fn a_failed_start_reaches_settings(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
    let _other = ipc::listen(&endpoint, std::sync::Arc::new(|_: &[u8]| Vec::new())).unwrap();
    set_preview(&tabs, true, cx);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.ai_connector = None;
        let vault = tabs.vault.vault().cloned().expect("the test Vault");
        let on = mdoc_store::settings::AiConnectorConfig {
            preview: true,
            ..Default::default()
        };
        let started = crate::ai_connector::start(vault, &on, Some(&endpoint));
        tabs.ai_started(started, window, cx);
        assert!(tabs.ai_connector.is_none());
    });
    let failed = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| (tabs.read(cx).settings.read(cx).ai_failed()).map(str::to_string))
    };
    assert!(
        failed(cx).is_some_and(|why| why.contains("another copy of mdoc")),
        "{:?}",
        failed(cx)
    );
    set_preview(&tabs, false, cx);
    assert_eq!(failed(cx), None);
}

#[gpui::test]
fn turning_the_preview_off_stops_the_connector(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
    set_preview(&tabs, true, cx);
    start(&tabs, &endpoint, cx);
    assert!(listening(&endpoint));
    // A Project shared, and the connector a call in flight would still hold.
    let held = tabs.update(cx, |tabs, _| {
        let vault = tabs.vault.vault().cloned().unwrap();
        let project = vault
            .create_project(mdoc_vault::NewProject {
                name: "Lease".into(),
                ..Default::default()
            })
            .unwrap();
        let connector = tabs.ai_connector.as_ref().unwrap().connector.clone();
        connector.share(&project.id).unwrap();
        connector
    });
    // Another setting saved leaves it running.
    set_preview(&tabs, true, cx);
    cx.update(|_, cx| assert!(tabs.read(cx).ai_connector.is_some()));
    assert!(held.ai_project().is_some());
    set_preview(&tabs, false, cx);
    cx.update(|_, cx| assert!(tabs.read(cx).ai_connector.is_none()));
    // Off the window's thread, a moment later (#1220).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while held.ai_project().is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "a held call still finds the Project shared"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // Turned on again at once, it listens again.
    start(&tabs, &endpoint, cx);
    assert!(listening(&endpoint));
    set_preview(&tabs, false, cx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while listening(&endpoint) {
        assert!(std::time::Instant::now() < deadline, "still listening");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Settings › AI connector open, its next save held held seconds, and
/// "AI connector (preview)" clicked, as a person turns it.
fn click_preview_held(tabs: &Entity<Workspace>, held: u64, cx: &mut VisualTestContext) {
    cx.simulate_resize(size(px(1100.), px(2400.)));
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.settings.update(cx, |panel, cx| {
            panel.show_category(settings_ui::Category::AiConnector, window, cx);
            let gate = (cx.background_executor()).timer(std::time::Duration::from_secs(held));
            panel.hold_next_save(cx.background_executor().spawn(gate));
        })
    });
    draw(cx);
    let bounds = (cx.debug_bounds("ai-connector-preview")).expect("the switch shows");
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
}

/// Settings publishes `event`, as a model check finishing (`Checked`) or
/// another save landing (`Applied`) does.
fn publish(tabs: &Entity<Workspace>, event: settings_ui::Event, cx: &mut VisualTestContext) {
    tabs.update(cx, |tabs, cx| {
        tabs.settings.update(cx, |_, cx| cx.emit(event))
    });
    cx.run_until_parked();
}

/// A minute passes: a save held that long lands.
fn save_lands(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(61));
    cx.run_until_parked();
}

fn saved_on(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| tabs.read(cx).ai_setting().preview)
}

/// No connector, and nothing at the endpoint for a shim to reach.
#[track_caller]
fn assert_stopped(tabs: &Entity<Workspace>, endpoint: &Endpoint, cx: &mut VisualTestContext) {
    cx.update(|_, cx| assert!(tabs.read(cx).ai_connector.is_none(), "the connector runs"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while listening(endpoint) {
        assert!(std::time::Instant::now() < deadline, "still listening");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Done when (#1273): the preview turned off, a model check finishing
/// before the save lands leaves the connector stopped, with Settings open
/// or closed, and so does another save landing meanwhile; off once saved,
/// it stays stopped.
#[gpui::test]
fn a_model_check_mid_save_leaves_off_off(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
    tabs.update(cx, |tabs, _| tabs.ai_endpoint = Some(endpoint.clone()));
    set_preview(&tabs, true, cx);
    assert!(listening(&endpoint));
    click_preview_held(&tabs, 60, cx);
    assert!(saved_on(&tabs, cx), "the save is still held");
    assert_stopped(&tabs, &endpoint, cx);
    publish(&tabs, settings_ui::Event::Checked, cx);
    assert_stopped(&tabs, &endpoint, cx);
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.settings
            .update(cx, |panel, cx| panel.close(window, cx))
    });
    publish(&tabs, settings_ui::Event::Checked, cx);
    assert_stopped(&tabs, &endpoint, cx);
    publish(&tabs, settings_ui::Event::Applied, cx);
    assert_stopped(&tabs, &endpoint, cx);
    save_lands(cx);
    assert!(!saved_on(&tabs, cx));
    publish(&tabs, settings_ui::Event::Checked, cx);
    assert_stopped(&tabs, &endpoint, cx);
}

/// #1273, the reverse: turned on with its save held, the connector starts
/// once the save lands, not before, and a model check after leaves the
/// one connector running.
#[gpui::test]
fn on_mid_save_starts_once(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
    tabs.update(cx, |tabs, _| tabs.ai_endpoint = Some(endpoint.clone()));
    set_preview(&tabs, false, cx);
    click_preview_held(&tabs, 60, cx);
    assert!(!saved_on(&tabs, cx), "the save is still held");
    publish(&tabs, settings_ui::Event::Checked, cx);
    assert_stopped(&tabs, &endpoint, cx);
    save_lands(cx);
    assert!(saved_on(&tabs, cx));
    assert!(listening(&endpoint));
    let first = cx.update(|_, cx| {
        tabs.read(cx)
            .ai_connector
            .as_ref()
            .map(|r| r.connector.clone())
    });
    let first = first.expect("started once saved");
    publish(&tabs, settings_ui::Event::Checked, cx);
    let now = cx.update(|_, cx| {
        tabs.read(cx)
            .ai_connector
            .as_ref()
            .map(|r| r.connector.clone())
    });
    assert!(
        now.is_some_and(|now| std::sync::Arc::ptr_eq(&now, &first)),
        "started again"
    );
    assert!(listening(&endpoint));
}

/// Review of #1322: off, then on again while the off is still saving: a
/// model check meanwhile starts nothing, the off landing leaves it
/// stopped, and the on starts it once its own save lands.
#[gpui::test]
fn off_then_on_mid_save_starts_once_the_on_lands(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
    tabs.update(cx, |tabs, _| tabs.ai_endpoint = Some(endpoint.clone()));
    set_preview(&tabs, true, cx);
    assert!(listening(&endpoint));
    click_preview_held(&tabs, 60, cx);
    // On again, its own save queued behind the off's and held longer.
    click_preview_held(&tabs, 120, cx);
    assert!(saved_on(&tabs, cx), "the off is still saving");
    publish(&tabs, settings_ui::Event::Checked, cx);
    assert_stopped(&tabs, &endpoint, cx);
    save_lands(cx);
    assert!(!saved_on(&tabs, cx), "the off landed");
    assert_stopped(&tabs, &endpoint, cx);
    save_lands(cx);
    assert!(saved_on(&tabs, cx), "the on landed");
    assert!(listening(&endpoint));
}
