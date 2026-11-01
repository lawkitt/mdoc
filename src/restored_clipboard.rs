//! Restored text on the clipboard (recognition grill 2026-11-01, round 3,
//! decision 2). A copy whose text comes from restored originals carries a
//! tag; the vendored platform crates then write it concealed and transient:
//! on macOS for this Mac only, with the nspasteboard.org Concealed and
//! Transient types; on Windows out of clipboard history, cloud sync and
//! clipboard monitors. The tag is ordinary clipboard metadata, so test and
//! headless platforms keep it without any OS call. Copy for AI and
//! Pseudonymize Clipboard copy aliases and stay normal copies.
use gpui::ClipboardItem;

#[cfg(target_os = "macos")]
pub(crate) use gpui_macos::CONCEALED_CLIPBOARD_METADATA as TAG;
#[cfg(windows)]
pub(crate) use gpui_windows::CONCEALED_CLIPBOARD_METADATA as TAG;
/// No shipped platform: the tag still marks the copy for tests.
#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) const TAG: &str = r#"{"mdoc":"concealed"}"#;

/// A clipboard item for restored text.
pub(crate) fn item(text: String) -> ClipboardItem {
    ClipboardItem::new_string_with_metadata(text, TAG.to_owned())
}

/// Whether `item` was written as restored text.
#[cfg(test)]
pub(crate) fn is_restored(item: &ClipboardItem) -> bool {
    item.metadata().is_some_and(|metadata| metadata == TAG)
}
