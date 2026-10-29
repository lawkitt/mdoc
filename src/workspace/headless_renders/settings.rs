//! Settings categories, categories to detect and the Spelling dictionary
//! (ADR 0076).
use super::{Driver, Result};
use crate::settings_ui::Category;

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
