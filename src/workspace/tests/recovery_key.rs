//! The recovery key (#1091 Decision 22) through the window, with an
//! injected Vault and key store: Settings › Vault shows it, the bell nudges
//! once after the first Project, and the wrong-key screen opens a Vault
//! sealed with another key and keeps that key.
use super::super::{recovery_key::InjectedKey, vault_state::Opening};
use super::*;
use mdoc_store::document_id::{DocumentId, VaultId};
use mdoc_vault::{DocumentOrigin, NewDocument, SourceKind, Vault, recovery};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const KEY: [u8; 32] = [3; 32];

fn key_store(key: Option<[u8; 32]>) -> Arc<Mutex<InjectedKey>> {
    Arc::new(Mutex::new(InjectedKey {
        key,
        ..InjectedKey::default()
    }))
}

fn panel(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<settings_ui::Panel> {
    cx.update(|_, cx| tabs.read(cx).settings.clone())
}

fn show_vault_settings(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) {
    let panel = panel(tabs, cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.show_category(settings_ui::Category::Vault, window, cx)
    });
    draw(cx);
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    draw(cx);
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn settings_vault_shows_the_recovery_key_and_its_qr_and_forgets_them(cx: &mut TestAppContext) {
    let store = key_store(Some(KEY));
    let (tabs, cx) = boot_home_with(cx, Session::default(), {
        let store = store.clone();
        move |tabs, _, _| {
            tabs.use_temp_vault();
            tabs.use_key_store(store);
        }
    });
    show_vault_settings(&tabs, cx);
    let panel = panel(&tabs, cx);
    // Nothing shows until asked.
    cx.update(|_, cx| assert_eq!(panel.read(cx).recovery_key_shown(), None));
    click(cx, "recovery-key-show");
    let shown = cx
        .update(|_, cx| panel.read(cx).recovery_key_shown())
        .unwrap();
    assert_eq!(shown, recovery::format(&KEY));
    assert_eq!(shown.split(' ').count(), 13);
    // The 64 characters shown, at level M, fit version 4: 33 modules a side.
    let width = cx
        .update(|_, cx| panel.read(cx).recovery_qr_width())
        .unwrap();
    assert_eq!(width, 33);
    // Hide, another category and closing Settings all drop it.
    click(cx, "recovery-key-hide");
    cx.update(|_, cx| assert_eq!(panel.read(cx).recovery_key_shown(), None));
    click(cx, "recovery-key-show");
    panel.update_in(cx, |panel, window, cx| {
        panel.show_category(settings_ui::Category::About, window, cx)
    });
    cx.update(|_, cx| assert_eq!(panel.read(cx).recovery_key_shown(), None));
    show_vault_settings(&tabs, cx);
    click(cx, "recovery-key-show");
    panel.update_in(cx, |panel, window, cx| panel.close(window, cx));
    cx.update(|_, cx| assert_eq!(panel.read(cx).recovery_key_shown(), None));
    // Nothing was stored, and the key never reached a notice.
    assert!(store.lock().unwrap().stored.is_empty());
    cx.update(|_, cx| assert!(tabs.read(cx).notice_text().is_none()));
}

#[gpui::test]
fn a_key_that_does_not_open_these_documents_is_never_shown(cx: &mut TestAppContext) {
    let (tabs, cx) = boot_home_with(cx, Session::default(), |tabs, _, _| {
        tabs.use_temp_vault();
        tabs.use_key_store(key_store(Some([9; 32])));
    });
    show_vault_settings(&tabs, cx);
    click(cx, "recovery-key-show");
    let panel = panel(&tabs, cx);
    cx.update(|_, cx| assert_eq!(panel.read(cx).recovery_key_shown(), None));
}

#[gpui::test]
fn the_first_project_brings_one_nudge_that_stays_closed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    tabs.update(cx, |tabs, _| tabs.use_key_store(key_store(Some(KEY))));
    draw(cx);
    cx.update(|_, cx| assert!(!tabs.read(cx).recovery_nudge()));
    // Move to New Project makes the first Project.
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    new_project(&tabs, cx, tab);
    draw(cx);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.recovery_nudge());
        // It opens the bell by itself.
        assert!(tabs.title.notices_open.get());
        assert!(tabs.title.notices.get() >= 1);
    });
    click(cx, "recovery-nudge-later");
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(!tabs.recovery_nudge());
        let preferences = tabs.preferences.borrow();
        assert!(
            (preferences.current.as_ref().unwrap())
                .general
                .recovery_key_nudge_dismissed
        );
    });
    // Closed for good: the next frame shows none.
    draw(cx);
    cx.update(|_, cx| assert!(!tabs.read(cx).recovery_nudge()));
    assert!(cx.debug_bounds("recovery-nudge").is_none());
}

#[gpui::test]
fn the_nudges_show_opens_settings_with_the_key_and_retires_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    tabs.update(cx, |tabs, _| tabs.use_key_store(key_store(Some(KEY))));
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    new_project(&tabs, cx, tab);
    draw(cx);
    click(cx, "recovery-nudge-show");
    let panel = panel(&tabs, cx);
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault()).unwrap();
    cx.update(|_, cx| {
        let panel = panel.read(cx);
        assert!(panel.open);
        assert_eq!(panel.category(), settings_ui::Category::Vault);
        assert!(panel.recovery_key_shown().is_some());
        assert!(!tabs.read(cx).recovery_nudge());
    });
    // The temporary Vault's key is the injected one shown.
    let shown = cx
        .update(|_, cx| panel.read(cx).recovery_key_shown())
        .unwrap();
    assert!(vault.opens_with(&recovery::parse(&shown).unwrap()));
}

#[gpui::test]
fn export_project_in_the_project_menu_writes_its_files(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (tabs, cx, _) = applied_document(cx, dir.path(), Session::default());
    let tab = cx.update(|_, cx| tabs.read(cx).tabs.active());
    let id = new_project(&tabs, cx, tab);
    let name = vault_of(&tabs, cx).project(&id).unwrap().name;
    click(cx, "project-header-menu");
    click(cx, "project-action-export");
    assert!(cx.did_prompt_for_paths());
    let picked = tempfile::tempdir().unwrap();
    let into = picked.path().to_path_buf();
    cx.simulate_path_prompt_response(move |_| Some(vec![into]));
    cx.run_until_parked();
    let folder = picked.path().join(&name);
    let markdown = std::fs::read_to_string(folder.join(format!("{name}.md"))).unwrap();
    assert_eq!(markdown, "PERSON_1 met ORG_1.");
    assert_eq!(
        std::fs::read_to_string(folder.join("Originals").join("contract.md")).unwrap(),
        "Anna Petrova met Northwind Traders Ltd."
    );
    let notice = cx.update(|_, cx| tabs.read(cx).notice_text()).unwrap();
    assert!(notice.starts_with("Exported 1 document"), "{notice}");
}

fn vault_of(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Arc<Vault> {
    cx.update(|_, cx| tabs.read(cx).opened_vault()).unwrap()
}

/// A Vault sealed with `sealed_with`, holding one Unfiled document, and a
/// session whose tab names it.
fn sealed_vault(dir: &Path, sealed_with: &[u8; 32]) -> (PathBuf, Session, String) {
    let root = dir.join("vault");
    let stored = Vault::open(&root, sealed_with)
        .unwrap()
        .create_document(NewDocument {
            project: None,
            title: "Lease".into(),
            source_kind: SourceKind::Text,
            origin: DocumentOrigin::Ordinary,
            markdown: "# Lease\n\nThe tenant pays rent.".into(),
        })
        .unwrap();
    let mut tab = TabRecord::default();
    tab.set_document(DocumentId::Vault(VaultId::parse(&stored.id).unwrap()));
    let session = Session {
        tabs: vec![tab],
        ..Session::default()
    };
    (root, session, stored.markdown)
}

/// Launch on `root` with this computer's key `own`, as the window does.
fn launch_with(
    root: PathBuf,
    own: [u8; 32],
    store: Arc<Mutex<InjectedKey>>,
) -> impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static {
    move |tabs, window, cx| {
        tabs.use_key_store(store);
        let opening = match Vault::open(&root, &own) {
            Ok(vault) => Opening::Opened(Box::new(vault)),
            Err(mdoc_vault::VaultError::WrongKey) => Opening::WrongKey(root),
            Err(error) => Opening::Vault(error),
        };
        tabs.vault_opened(opening, window, cx);
    }
}

fn type_key(tabs: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.type_recovery_key(text, window, cx)
    });
}

fn screen_error(tabs: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| tabs.read(cx).recovery.error.clone())
}

#[gpui::test]
fn a_vault_copied_to_a_fresh_profile_opens_with_its_recovery_key(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let sealed_with = [1; 32];
    let (root, session, markdown) = sealed_vault(dir.path(), &sealed_with);
    // The fresh profile's own key is another one.
    let store = key_store(Some([2; 32]));
    let (tabs, cx) = boot_home_with(cx, session, launch_with(root, [2; 32], store.clone()));
    draw(cx);
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.vault.vault().is_none());
        let (line, action) = tabs.vault_banner().unwrap();
        assert!(line.contains("recovery key"), "{line}");
        assert_eq!(action, None);
    });
    assert!(cx.debug_bounds("home-recovery").is_some());
    // The restored tab waits; nothing can be added meanwhile.
    assert_eq!(
        active(&tabs, cx).read_with(cx, |view, cx| view.editor.read(cx).text().to_string()),
        ""
    );
    tabs.update_in(cx, |tabs, window, cx| {
        assert!(tabs.intake_refused(window, cx));
        assert!(tabs.notice_text().unwrap().contains("recovery key"));
        tabs.notices.dismiss();
    });

    // Not a recovery key: said at once, the text stays to fix.
    type_key(&tabs, "ABCD EFGH", cx);
    click(cx, "home-recovery-open");
    assert!(screen_error(&tabs, cx).unwrap().contains("52 characters"));
    // Another computer's key doesn't open them.
    type_key(&tabs, &recovery::format(&[7; 32]), cx);
    cx.update(|_, cx| shell::bind_keys(cx));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        screen_error(&tabs, cx)
            .unwrap()
            .contains("doesn't open these documents")
    );
    assert!(store.lock().unwrap().stored.is_empty());

    // The right key, typed in lower case with dashes, opens them.
    let typed = recovery::format(&sealed_with)
        .to_lowercase()
        .replace(' ', "-");
    type_key(&tabs, &typed, cx);
    click(cx, "home-recovery-open");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let tabs = tabs.read(cx);
        assert!(tabs.vault.vault().is_some());
        assert!(tabs.vault_banner().is_none());
        assert!(tabs.recovery.field.is_none() && tabs.recovery.error.is_none());
        assert!(tabs.notice_text().is_none());
    });
    assert!(cx.debug_bounds("home-recovery").is_none());
    assert_eq!(
        active(&tabs, cx).read_with(cx, |view, cx| view.editor.read(cx).text().to_string()),
        markdown
    );
    // Kept once, as this computer's Vault key: the next launch opens.
    let stored = store.lock().unwrap().stored.clone();
    assert_eq!(stored, vec![sealed_with]);
    let root = dir.path().join("vault");
    assert!(Vault::open(&root, &store.lock().unwrap().key.unwrap()).is_ok());
}

#[gpui::test]
fn a_key_the_os_refuses_to_keep_still_opens_this_run_and_says_so(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let sealed_with = [4; 32];
    let (root, session, markdown) = sealed_vault(dir.path(), &sealed_with);
    let store = key_store(Some([2; 32]));
    store.lock().unwrap().refuse = true;
    let (tabs, cx) = boot_home_with(cx, session, launch_with(root, [2; 32], store.clone()));
    type_key(&tabs, &recovery::format(&sealed_with), cx);
    click(cx, "home-recovery-open");
    cx.run_until_parked();
    assert_eq!(
        active(&tabs, cx).read_with(cx, |view, cx| view.editor.read(cx).text().to_string()),
        markdown
    );
    let notice = cx.update(|_, cx| tabs.read(cx).notice_text()).unwrap();
    assert!(
        notice.contains("asks for it at the next launch"),
        "{notice}"
    );
    assert!(!notice.contains(&recovery::format(&sealed_with)[..4]));
    assert_eq!(store.lock().unwrap().key, Some([2; 32]));
}
