//! The AI project in the window (Milestone 2 item 8), with "AI connector
//! (preview)" on: the share prompt after an AI app asked, the indicator
//! with the Project shared (the sidebar's foot, the Project page's chip
//! and the collapsed rail), and the Project page's AI activity. The AI
//! app's calls go through the connector's own handler, with no listener.
//! It leaves the setting off and the fixture as it found it.
use super::{Driver, Result};
use crate::workspace::project_page::{Place, Segment};
use mdoc_mcp::wire::{self, Answer, Call, Kind, Request};

/// AI app `app` makes `call`; the window hears of it as its poll would.
fn call(d: &mut Driver, app: &str, call: Call) -> Result<Answer> {
    d.change(|w, window, cx| {
        let running = w.ai_connector.as_ref().ok_or("the connector runs")?;
        let answer = running.call(&Request {
            v: wire::VERSION,
            app: app.into(),
            call,
        });
        w.poll_ai_connector(window, cx);
        Ok(answer)
    })?
}

/// Captured with no control focused, so no focus ring shows.
fn capture(d: &mut Driver, name: &str) -> Result<()> {
    d.change(|w, window, cx| window.focus(&w.focus, cx))?;
    d.capture(name)
}

fn preview(d: &mut Driver, on: bool) -> Result<()> {
    d.change(|w, window, cx| {
        if let Some(preferences) = w.preferences.borrow_mut().current.as_mut() {
            preferences.ai_connector.preview = on;
        }
        w.refresh_ai_connector(window, cx);
    })?;
    d.settle()
}

pub(super) fn scenes(d: &mut Driver, project: &str) -> Result<()> {
    // Where the window was, to go back there after.
    let was = d.change(|w, _, cx| (w.project_ui.page.is_some(), w.showing_home(cx)))?;
    let sidebar = d.change(|w, window, cx| {
        let shown = w.sidebar.visible();
        w.sidebar.restore(true, Some(true));
        w.show_project_page(project, Place::Header, window, cx);
        window.focus(&w.focus, cx);
        shown
    })?;
    preview(d, true)?;
    // A Ready member for the AI app to read.
    let witness = d.change(|w, _, _| -> Result<String> {
        let vault = w.vault.vault().cloned().ok_or("fixture Vault")?;
        let witness = (vault.documents_in(Some(project)).into_iter())
            .find(|summary| summary.title == "Witness statement")
            .ok_or("the fixture's witness statement")?;
        vault
            .update_document(&witness.id, |d| {
                d.scanned = true;
                d.set_ready(true);
            })
            .map_err(|e| e.to_string())?;
        Ok(witness.id)
    })??;
    // Claude asks with nothing shared: the prompt, this Project first.
    call(d, "claude-ai", Call::ListDocuments)?;
    capture(d, "ai-share-prompt")?;
    d.change(|w, _, cx| {
        if let Some(project) = w.ai.take_choice() {
            w.share_with_ai(&project, cx);
        }
    })?;
    capture(d, "ai-project-shared")?;
    // Reads and a delivery, then the page's AI activity.
    let read = || Call::GetDocument {
        doc_id: witness.clone(),
        part: None,
    };
    call(d, "claude-ai", read())?;
    let delivered = call(
        d,
        "claude-ai",
        Call::Deliver {
            kind: Kind::Reply,
            title: Some("Public statement summary".into()),
            read: vec![witness.clone()],
            markdown: "The statement is public.".into(),
        },
    )?;
    call(d, "openai-chatgpt", read())?;
    d.change(|w, _, cx| w.show_project_segment(Segment::Activity, cx))?;
    capture(d, "project-page-activity")?;
    d.change(|w, _, cx| {
        w.sidebar.restore(false, Some(false));
        cx.notify();
    })?;
    capture(d, "ai-project-rail")?;
    // As it was: the delivered reply gone, the member not Ready, the
    // setting off and the sidebar as it was.
    d.change(|w, _, cx| -> Result<()> {
        let vault = w.vault.vault().cloned().ok_or("fixture Vault")?;
        if let Answer::Ok(wire::Done::Delivered { reply, .. }) = delivered {
            vault.delete_reply(&reply).map_err(|e| e.to_string())?;
        }
        vault
            .update_document(&witness, |d| {
                d.scanned = false;
                d.set_ready(false);
            })
            .map_err(|e| e.to_string())?;
        w.show_project_segment(Segment::Documents, cx);
        w.sidebar.restore(sidebar, Some(sidebar));
        Ok(())
    })??;
    d.change(|w, window, cx| match was {
        (true, _) => {}
        (false, true) => w.show_home(window, cx),
        (false, false) => w.activate(w.tabs.active(), window, cx),
    })?;
    preview(d, false)
}
