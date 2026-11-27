//! Saving: a save with proposals pending here, and the DOCX queue, the
//! session writer and the live views in their own files.
pub(super) mod docx_queue;
pub(super) mod live_views;
pub(super) mod session_writer;
use super::*;

/// Pending proposals never block a save, and saving never applies them: the
/// file gets the text as it stands and the proposals stay pending.
#[gpui::test]
fn saving_with_pending_proposals_writes_the_text_and_keeps_them(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pending.md");
    let source = "Anna Petrova met Northwind Traders Ltd.";
    std::fs::write(&path, source).unwrap();
    let (tabs, cx) = boot_home(cx, Session::default());
    tabs.update_in(cx, |tabs, window, cx| {
        tabs.open_paths(vec![path.clone()], window, cx)
    });
    cx.run_until_parked();
    let view = active(&tabs, cx);
    let pending = view.update_in(cx, |view, _, cx| {
        view.pii.set_reviewing(true);
        view.pii
            .review_mut()
            .add_manual(source, 0..12, mdoc_pii::Category::Person)
            .unwrap();
        let pending = view.pii.review().remaining();
        assert!(pending > 0);
        view.editor.update(cx, |editor, cx| {
            editor.replace_range(source.len()..source.len(), "\n\nA note.", cx)
        });
        view.save_now(cx);
        pending
    });
    cx.run_until_parked();
    let document = view.read_with(cx, |view, _| view.session.document.vault.clone().unwrap());
    let vault = cx.update(|_, cx| tabs.read(cx).opened_vault().unwrap());
    assert_eq!(
        vault.document(document.as_str()).unwrap().markdown,
        format!("{source}\n\nA note.")
    );
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.pii.review().remaining(), pending);
        assert!(view.pii.review().applied().is_empty());
    });
}
