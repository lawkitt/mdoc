//! The recovery key in the window (#1091 Decision 22): Settings › Vault's
//! Show reads the key from the OS store; the first-Project nudge in the
//! bell offers it once; and when the Vault was sealed with another key,
//! Home asks for the recovery key, opens the Vault with it and keeps it
//! in the OS store as the Vault key. mdoc never writes the key to a file,
//! a log or the clipboard. Tests and headless renders inject the key
//! store, so they never ask the OS.
use super::{Workspace, vault_state::Opening};
use crate::{document::markdown_search::SearchInput, settings::Category, ui, ui::style};
use gpui::{AnyElement, Context, Entity, Focusable, Window, actions, div, prelude::*, px};
use mdoc_vault::{Vault, VaultError, recovery};
use zeroize::Zeroizing;

actions!(workspace, [OpenWithRecoveryKey]);

/// The key context of the wrong-key screen's field.
pub(crate) const RECOVERY_FIELD: &str = "RecoveryKey";

/// Where the window reads and keeps the Vault key.
#[derive(Clone, Default)]
pub(crate) enum KeyStore {
    /// The OS key store, through mdoc-store's guarded key path.
    #[default]
    Os,
    /// Tests and headless renders: a key in memory.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    Injected(std::sync::Arc<std::sync::Mutex<InjectedKey>>),
}

/// An injected key store: what it holds, and what was stored in it.
#[cfg(any(
    test,
    all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    )
))]
#[derive(Default)]
pub(crate) struct InjectedKey {
    pub key: Option<[u8; 32]>,
    /// Every key stored, in order.
    pub stored: Vec<[u8; 32]>,
    /// Storing fails, as when the OS refuses.
    pub refuse: bool,
}

impl KeyStore {
    /// The Vault key, read the way Unlock reads it (it may ask on macOS).
    fn read(&self) -> Result<Zeroizing<[u8; 32]>, String> {
        match self {
            Self::Os => match mdoc_store::os_key::vault_key(false) {
                Ok(Some(key)) => Ok(Zeroizing::new(key)),
                Ok(None) => Err("mdoc could not read the key without asking.".into()),
                Err(error) => Err(error.to_string()),
            },
            #[cfg(any(
                test,
                all(
                    debug_assertions,
                    feature = "headless-renders",
                    any(target_os = "macos", target_os = "windows")
                )
            ))]
            Self::Injected(store) => (store.lock().unwrap().key)
                .map(Zeroizing::new)
                .ok_or_else(|| "No key is held.".into()),
        }
    }

    /// Keep `key` as the Vault key.
    fn store(&self, key: &[u8; 32]) -> Result<(), String> {
        match self {
            Self::Os => mdoc_store::os_key::store_vault_key(key).map_err(|e| e.to_string()),
            #[cfg(any(
                test,
                all(
                    debug_assertions,
                    feature = "headless-renders",
                    any(target_os = "macos", target_os = "windows")
                )
            ))]
            Self::Injected(store) => {
                let mut store = store.lock().unwrap();
                if store.refuse {
                    return Err("The key store refused it.".into());
                }
                store.key = Some(*key);
                store.stored.push(*key);
                Ok(())
            }
        }
    }
}

/// The wrong-key screen's state.
#[derive(Default)]
pub(super) struct Recovery {
    pub(super) store: KeyStore,
    pub(super) field: Option<Entity<SearchInput>>,
    /// Why the last key typed didn't open the Vault.
    pub(super) error: Option<String>,
    /// A typed key is being tried.
    opening: bool,
}

impl Workspace {
    /// Settings › Vault's Show: the key, when it opens these documents.
    pub(super) fn show_recovery_key(&mut self, cx: &mut Context<Self>) {
        let key = match self.vault.vault() {
            None => Err("Your recovery key shows here once your documents are open.".to_string()),
            Some(vault) => self.recovery.store.read().and_then(|key| {
                if vault.opens_with(&key) {
                    Ok(*key)
                } else {
                    Err(
                        "The key on this computer doesn't open these documents. Quit mdoc \
                         and open it again to enter your recovery key."
                            .into(),
                    )
                }
            }),
        };
        if key.is_ok() {
            self.dismiss_recovery_nudge(cx);
        }
        self.settings
            .update(cx, |panel, cx| panel.show_recovery_key(key, cx));
    }

    /// Tests and headless renders: the key store the window reads.
    #[cfg(any(
        test,
        all(
            debug_assertions,
            feature = "headless-renders",
            any(target_os = "macos", target_os = "windows")
        )
    ))]
    pub(crate) fn use_key_store(&mut self, store: std::sync::Arc<std::sync::Mutex<InjectedKey>>) {
        self.recovery.store = KeyStore::Injected(store);
    }

    // ----- The first-Project nudge -----

    /// The nudge shows in the bell once the Vault holds a Project, until
    /// it is closed or the key is shown.
    pub(super) fn recovery_nudge(&self) -> bool {
        let dismissed = (self.preferences.borrow().current.as_ref())
            .is_none_or(|preferences| preferences.general.recovery_key_nudge_dismissed);
        !dismissed && self.vault.vault().is_some() && !self.projects.is_empty()
    }

    pub(super) fn dismiss_recovery_nudge(&mut self, cx: &mut Context<Self>) {
        if !self.recovery_nudge() {
            return;
        }
        if let Err(error) =
            self.update_general(|general| general.recovery_key_nudge_dismissed = true, cx)
        {
            self.notices.post(format!(
                "Could not save that the reminder was closed: {error}"
            ));
        }
        cx.notify();
    }

    /// The nudge's card in the bell.
    pub(super) fn recovery_nudge_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.recovery_nudge() {
            return None;
        }
        let theme = self.theme.get();
        let accent = theme.search_accent();
        Some(
            div()
                .id("recovery-nudge")
                .when(cfg!(test), |v| v.debug_selector(|| "recovery-nudge".into()))
                .flex()
                .flex_col()
                .gap_2()
                .px_3()
                .py_2()
                .text_size(px(12.))
                .bg(gpui::Hsla { a: 0.07, ..accent })
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(div().text_color(accent).child("ⓘ"))
                        .child(div().flex_1().min_w_0().child(
                            "Save your recovery key. It's the only way to open your documents \
                             if this computer's key is lost, as after reinstalling the system.",
                        )),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            ui::control("recovery-nudge-later", "Not Now", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "recovery-nudge-later".into())
                                })
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.dismiss_recovery_nudge(cx)),
                                ),
                        )
                        .child(
                            ui::control("recovery-nudge-show", "Show Recovery Key", theme, true)
                                .when(cfg!(test), |v| {
                                    v.debug_selector(|| "recovery-nudge-show".into())
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.title.notices_open.set(false);
                                    this.settings.update(cx, |panel, cx| {
                                        panel.show_category(Category::Vault, window, cx)
                                    });
                                    this.show_recovery_key(cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    // ----- The wrong-key screen -----

    /// Home's drop zone while the Vault was sealed with another key: the
    /// recovery key field and Open Documents.
    pub(super) fn recovery_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        let field = self.recovery.field.clone();
        div()
            .id("home-recovery")
            .when(cfg!(test), |v| v.debug_selector(|| "home-recovery".into()))
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .px_6()
            .pt_6()
            .pb_5()
            .rounded_xl()
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(p.header_fg)
                    .child("Enter your recovery key"),
            )
            .child(
                div()
                    .max_w(px(460.))
                    .text_center()
                    .text_size(px(12.))
                    .text_color(p.header_muted)
                    .child(
                        "Your documents were saved with a different key, as after reinstalling \
                         the system or copying them from another computer. The recovery key \
                         from Settings › Vault on that computer opens them, and this computer \
                         keeps it.",
                    ),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(680.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .border_1()
                            .border_color(p.border)
                            .font_family(style::markdown_style(theme).mono.family.clone())
                            .key_context(RECOVERY_FIELD)
                            .on_action(cx.listener(|this, _: &OpenWithRecoveryKey, window, cx| {
                                this.open_with_recovery_key(window, cx)
                            }))
                            .children(field),
                    )
                    .child(
                        ui::filled_button(
                            "home-recovery-open",
                            "Open Documents",
                            theme,
                            !self.recovery.opening,
                        )
                        .when(cfg!(test), |v| {
                            v.debug_selector(|| "home-recovery-open".into())
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_with_recovery_key(window, cx)
                        })),
                    ),
            )
            .children(self.recovery.error.clone().map(|error| {
                div()
                    .when(cfg!(test), |v| {
                        v.debug_selector(|| "home-recovery-error".into())
                    })
                    .max_w(px(460.))
                    .text_center()
                    .text_size(px(12.))
                    .text_color(style::markdown_style(theme).alert_caution)
                    .child(error)
            }))
            .into_any_element()
    }

    /// The Vault turned out sealed with another key: Home shows the screen,
    /// its field focused, over restored tabs that wait for the key.
    pub(super) fn recovery_screen_opened(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.wrong_key_root().is_none() {
            return;
        }
        // At launch, restore shows it once the tabs are back.
        if self.ready {
            self.show_home(window, cx);
        }
        let field = self.recovery_field(window, cx);
        let focus = field.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    fn recovery_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SearchInput> {
        if let Some(field) = &self.recovery.field {
            return field.clone();
        }
        let theme = self.theme.clone();
        let field = cx.new(|cx| {
            SearchInput::new(cx)
                .with_theme(theme)
                .with_key_context(RECOVERY_FIELD)
                .with_placeholder("ABCD EFGH IJKL …")
                .with_label("Recovery key")
        });
        let focus = field.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.recovery.field = Some(field.clone());
        field
    }

    /// Tests: type `text` into the wrong-key screen's field.
    #[cfg(test)]
    pub(crate) fn type_recovery_key(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = self.recovery_field(window, cx);
        field.update(cx, |field, cx| field.set_value(text.into(), cx));
        let focus = field.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// Headless renders: the wrong-key screen with a key typed and refused.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn refuse_recovery_key_for_render(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = self.recovery_field(window, cx);
        field.update(cx, |field, cx| field.set_value(text.into(), cx));
        self.recovery.error = Some(refused(&VaultError::WrongKey));
        cx.notify();
    }

    /// Headless renders: the Vault opened again, so the screen goes.
    #[cfg(all(
        debug_assertions,
        feature = "headless-renders",
        any(target_os = "macos", target_os = "windows")
    ))]
    pub(super) fn recovery_screen_closed_for_render(&mut self) {
        self.recovery.field = None;
        self.recovery.error = None;
    }

    /// Open Documents: read the typed key, open the Vault with it off the
    /// main thread and, once it opens, keep it as the Vault key.
    pub(super) fn open_with_recovery_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(root), Some(field)) = (self.vault.wrong_key_root(), self.recovery.field.clone())
        else {
            return;
        };
        if self.recovery.opening {
            return;
        }
        let key = match recovery::parse(field.read(cx).value()) {
            Ok(key) => Zeroizing::new(key),
            Err(error) => {
                self.recovery.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let root = root.to_path_buf();
        let store = self.recovery.store.clone();
        self.recovery.opening = true;
        self.recovery.error = None;
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let vault = Vault::open(root, &key)?;
            // Kept only once it has opened the documents.
            Ok::<_, VaultError>((vault, store.store(&key)))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.recovery.opening = false;
                match result {
                    Ok((vault, kept)) => {
                        if let Some(field) = this.recovery.field.take() {
                            field.update(cx, |field, cx| drop(field.take_secret(cx)));
                        }
                        this.recovery.error = None;
                        // Documents can be added now.
                        this.notices
                            .withdraw(super::vault_ui::WAITS_FOR_RECOVERY_KEY);
                        if let Err(why) = kept {
                            this.notices.post(format!(
                                "Your documents are open, but this computer couldn't keep the \
                                 recovery key, so mdoc asks for it at the next launch. {why}"
                            ));
                        }
                        this.vault_opened(Opening::Opened(Box::new(vault)), window, cx);
                    }
                    Err(error) => {
                        this.recovery.error = Some(refused(&error));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

fn refused(error: &VaultError) -> String {
    match error {
        VaultError::WrongKey => {
            "That recovery key doesn't open these documents. Check each group and try again.".into()
        }
        error => format!("mdoc could not open the documents with it. {error}"),
    }
}
