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
        tabs.ai_connector = crate::ai_connector::start(vault, &on, endpoint);
        assert!(tabs.ai_connector.is_some(), "listening at once");
    });
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
    assert_eq!(held.ai_project(), None, "a held call finds nothing shared");
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
