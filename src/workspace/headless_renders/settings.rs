//! Settings categories, categories to detect, the user-wide lists (ADR
//! 0092) and the Spelling dictionary (ADR 0076).
use super::{Driver, Result};
use crate::settings_ui::{Category, Status};

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    // show_settings activates native windows. Call the panel's state flow
    // directly, even though TestPlatform cannot show a native window.
    for (category, name) in [
        (Category::General, "settings-general"),
        (Category::Detection, "settings-detection"),
        (Category::Recognition, "settings-recognition"),
        (Category::Spelling, "settings-spelling"),
        (Category::Cases, "settings-cases"),
        (Category::About, "settings-about"),
    ] {
        d.change(|w, window, cx| {
            w.settings
                .update(cx, |p, cx| p.show_category(category, window, cx));
        })?;
        // Each baseline precedes its Tab walk, including Appearance in General.
        d.capture(name)?;
    }
    // ADR 0076: switches share one column and an off switch is grey. General
    // with the switches off, unsaved; Include pre-releases stays on in a
    // pre-release build.
    d.change(|w, window, cx| {
        w.settings.update(cx, |p, cx| {
            p.show_category(Category::General, window, cx);
            p.render_draft(
                |p| {
                    p.updates.check_automatically = false;
                    p.general.menu_bar = false;
                },
                cx,
            );
        });
    })?;
    d.capture("settings-general-off")?;
    d.change(|w, _, cx| w.settings.update(cx, |p, cx| p.render_draft(|_| {}, cx)))?;
    // The selected Appearance segment is filled; focus on another adds only
    // its ring. Neither run selects System.
    d.change(|w, window, cx| {
        w.settings
            .update(cx, |p, cx| p.show_category(Category::General, window, cx));
    })?;
    d.settle()?;
    let system = d
        .change(|w, _, cx| w.settings.read(cx).control_focus("settings-theme-system"))?
        .ok_or("System segment has no focus handle")?;
    d.focus_pair("settings-appearance", system)?;
    // Installed models: the active and an inactive model each offer a
    // bordered Remove….
    let before = d.change(|w, window, cx| {
        w.settings.update(cx, |p, cx| {
            p.show_category(Category::Detection, window, cx);
            p.render_statuses(vec![Status::Ready; p.statuses.len()], cx)
        })
    })?;
    d.capture("settings-models-installed")?;
    d.change(|w, _, cx| {
        w.settings.update(cx, |p, cx| {
            p.render_statuses(before, cx);
        })
    })?;
    // A long page at the end of its scroll: the thumb at the bottom and the
    // fade above, where rows continue.
    d.change(|w, window, cx| {
        w.settings
            .update(cx, |p, cx| p.show_category(Category::Detection, window, cx));
    })?;
    d.settle()?;
    d.change(|w, _, cx| w.settings.update(cx, |p, cx| p.render_scroll_to_end(cx)))?;
    d.capture("settings-detection-end")?;
    // Categories to detect with Address and Email off; focus on a chip
    // scrolls the row into view and shows its ring (ADR 0076).
    d.change(|w, window, cx| {
        w.settings.update(cx, |p, cx| {
            p.show_category(Category::Detection, window, cx);
            p.render_categories(
                &[mdoc_pii::Category::Address, mdoc_pii::Category::Email],
                "PHONE",
                window,
                cx,
            )
        });
    })?;
    d.settle()?;
    d.capture("settings-detection-categories")?;
    d.change(|w, window, cx| {
        w.settings
            .update(cx, |p, cx| p.render_categories(&[], "", window, cx))
    })?;
    // The user-wide Always and Never lists, a wording typed to add, then
    // the same section while cases are locked (ADR 0092).
    d.change(|w, window, cx| {
        w.settings.update(cx, |p, cx| {
            p.show_category(Category::Detection, window, cx);
            let mut lists = mdoc_store::lists::Lists::default();
            lists.add_always("Acme Holdings", mdoc_pii::Category::Organization);
            lists.add_never("Supreme Court");
            lists.add_never("Ministry of Justice");
            p.set_user_lists(Some(lists), cx);
            p.type_user_list_entry(true, "Riverside Clinic", cx);
        })
    })?;
    d.settle()?;
    d.change(|w, _, cx| w.settings.update(cx, |p, cx| p.render_scroll_to_end(cx)))?;
    d.capture("settings-detection-lists")?;
    d.change(|w, _, cx| w.settings.update(cx, |p, cx| p.set_user_lists(None, cx)))?;
    d.settle()?;
    d.change(|w, _, cx| w.settings.update(cx, |p, cx| p.render_scroll_to_end(cx)))?;
    d.capture("settings-detection-lists-locked")?;
    d.change(|w, _, cx| {
        w.settings.update(cx, |p, cx| {
            p.type_user_list_entry(true, "", cx);
            p.set_user_lists(Some(Default::default()), cx)
        })
    })?;
    // Detection starts on its category tab; one Tab enters the first model radio.
    d.change(|w, window, cx| {
        w.settings
            .update(cx, |p, cx| p.show_category(Category::Detection, window, cx));
    })?;
    d.settle()?;
    d.key("tab")?;
    d.settle()?;
    let row = d.change(|_, window, cx| window.focused(cx).expect("first Settings row"))?;
    d.focus_pair("settings-row", row)?;
    d.change(|w, window, cx| w.settings.update(cx, |p, cx| p.open_dictionary(window, cx)))?;
    d.capture("dictionary")?;
    d.key("escape")?;
    d.change(|w, window, cx| w.settings.update(cx, |p, cx| p.close(window, cx)))?;
    Ok(())
}
