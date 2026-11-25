//! Settings › AI connector's switch reaches the window: turned off, the
//! connector and its listener stop at once, without a relaunch.
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

#[gpui::test]
fn turning_the_preview_off_stops_the_connector(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home(cx, Session::default());
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::for_profile(Some(&dir.path().join("profile"))).unwrap();
    set_preview(&tabs, true, cx);
    // Tests never take the profile's endpoint, so the connector is started
    // here, on a test endpoint, as launch would with the setting on.
    tabs.update(cx, |tabs, _| {
        let vault = tabs.vault.vault().cloned().expect("the test Vault");
        let on = mdoc_store::settings::AiConnectorConfig { preview: true };
        tabs.ai_connector = crate::ai_connector::start(vault, &on, &endpoint);
        assert!(tabs.ai_connector.is_some());
    });
    assert!(listening(&endpoint));
    // Another setting saved leaves it running.
    set_preview(&tabs, true, cx);
    cx.update(|_, cx| assert!(tabs.read(cx).ai_connector.is_some()));
    set_preview(&tabs, false, cx);
    cx.update(|_, cx| assert!(tabs.read(cx).ai_connector.is_none()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while listening(&endpoint) {
        assert!(std::time::Instant::now() < deadline, "still listening");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
