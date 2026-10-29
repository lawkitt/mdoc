//! Restoring an AI reply: the Reply view beside Share (ADR 0078) and a
//! restored reply from the public Claude fixture.
use super::{Driver, Result};
use crate::document_view::DocumentView;
use gpui::Entity;
use serde_json::Value;
use std::sync::Arc;

/// A reply with one invented alias, in the Reply view beside Share, then
/// Share alone.
pub(super) fn reply_view(d: &mut Driver, view: &Entity<DocumentView>) -> Result<()> {
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.inspector_share = true;
            if !v.replacements_open() {
                v.toggle_replacements_panel(window, cx);
            }
            cx.notify();
        })
    })?;
    d.settle()?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            let alias = v.pii.review().export_identities()[0].alias.clone();
            let reply = format!(
                "## Summary\n\n{alias} keeps the deposit. PERSON_99 is not named in \
                 the document, so the reply view leaves it marked.\n"
            );
            v.fill_reply(&reply, window, cx);
        })
    })?;
    d.capture("reply-view")?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.set_view_mode(None, window, cx);
            // Start this Share Tab walk in the panel. The document editor's
            // Tab indents text and intentionally stays in its own region.
            window.focus(&v.replacements_focus(), cx);
        })
    })?;
    d.capture("share-reply")?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            v.fill_reply("", window, cx);
            v.inspector_share = false;
            cx.notify();
        })
    })?;
    Ok(())
}

/// A restored reply in a new tab, from the public Claude EN fixture.
pub(super) fn restored_reply(d: &mut Driver) -> Result<()> {
    let mapping: Vec<Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/restore/claude-en/identities.json"
    ))
    .map_err(|e| e.to_string())?;
    let identities = mapping
        .iter()
        .map(|identity| {
            Ok(mdoc_pii::restore::ExportedIdentity {
                alias: identity["alias"].as_str().ok_or("fixture alias")?.into(),
                previous: vec![],
                custom: false,
                category: mdoc_pii::Category::ALL
                    .into_iter()
                    .find(|c| Some(c.token()) == identity["category"].as_str())
                    .ok_or("fixture category")?,
                originals: identity["originals"]
                    .as_array()
                    .ok_or("fixture originals")?
                    .iter()
                    .map(|v| v.as_str().map(Arc::from).ok_or("fixture original"))
                    .collect::<std::result::Result<_, _>>()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let reply = crate::pii::ui::RestoredReply {
        text: include_str!("../../../tests/fixtures/restore/claude-en/reply.md").into(),
        identities,
        source_name: "Claude public EN fixture".into(),
        case: None,
    };
    d.change(|w, window, cx| {
        w.new_tab(window, cx);
        w.active_view()
            .unwrap()
            .update(cx, |v, cx| v.start_restored(&reply, window, cx));
    })?;
    d.capture("restored-reply")
}
