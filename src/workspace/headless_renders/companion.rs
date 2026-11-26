//! The companion window (Milestone 2 item 14): nothing shared; a Project
//! shared with documents in each state; the latest reply hidden, then
//! shown with the pointer over it; a review request; and empty. Each is
//! drawn on a stage of the usual size at the top right, as it sits under
//! the menu bar item, from content as the window reads it, with public,
//! synthetic names; no status item, panel or blur is involved.
use super::{Driver, HEIGHT, Result, WIDTH, shelf};
use crate::{
    companion::{COMPANION_WIDTH, CompanionView},
    style::Theme,
    workspace::{
        CompanionContent, Workspace,
        ai_companion::{CompanionDocument, CompanionProject, CompanionReply, CompanionReview},
    },
};
use gpui::{App, Entity, Render, Window, div, prelude::*, px, size};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc, time::Duration};

/// The companion as it sits on screen, for renders only.
pub(super) struct Stage {
    companion: Entity<CompanionView>,
    theme: Rc<Cell<Theme>>,
    height: f32,
}

impl Stage {
    pub(super) fn state(&self, cx: &App) -> Value {
        let view = self.companion.read(cx);
        let content = view.content();
        json!({
            "companion": true,
            "shared": content.shared.as_ref().map(|project| project.name.clone()),
            "documents": content.documents.as_ref().ok().map(|documents| {
                documents.iter().map(|d| json!({"label": d.label, "state": d.state})).collect::<Vec<_>>()
            }),
            "reply": content.reply.is_some(),
            "reply_shown": view.reply_shown(),
            "review": content.review.as_ref().map(|review| review.label.clone()),
            "rows": content.rows.len(),
        })
    }
}

impl Render for Stage {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        // The panel's corners and shadow are the system's on screen; an
        // opaque ground stands in for the blur behind it.
        div().size_full().bg(theme.sidebar_bg()).child(
            div()
                .absolute()
                .top(px(6.))
                .right(px(160.))
                .w(px(COMPANION_WIDTH))
                .h(px(self.height))
                .rounded(px(12.))
                .border_1()
                .border_color(p.border)
                .shadow_lg()
                .overflow_hidden()
                .bg(p.bg)
                .child(self.companion.clone()),
        )
    }
}

const PROJECT: &str = "Petrova v Northwind";

fn project() -> CompanionProject {
    CompanionProject {
        id: "c".repeat(32),
        name: PROJECT.into(),
        hue: 210,
    }
}

fn document(n: usize, label: &str, state: &str) -> CompanionDocument {
    CompanionDocument {
        id: format!("{:032x}", 0xd0 + n),
        label: label.into(),
        ready: state == "Ready",
        state: state.into(),
    }
}

/// A shared Project with a document in each state.
fn documents() -> Vec<CompanionDocument> {
    vec![
        document(0, "Services Agreement", "Ready"),
        document(1, "Witness statement", "Ready"),
        document(2, "Document 3", "Needs review · 2 names not replaced"),
        document(3, "Document 4", "Needs review · Changed since review"),
        document(4, "Document 5", "Needs review · Not reviewed"),
    ]
}

fn reply() -> CompanionReply {
    CompanionReply {
        id: "e".repeat(32),
        from: "Claude".into(),
        restored: "Anna Petrova may end the services agreement with Northwind Traders Ltd \
                   on 30 days' written notice under clause 9. Boris Ivanov signs for \
                   Northwind Traders Ltd, so the notice goes to him at \
                   boris.ivanov@example.com.\n\nNothing in the witness statement changes \
                   this."
            .into(),
        saved: false,
        saving: false,
        failed: false,
    }
}

fn content(shared: bool, rows: bool) -> CompanionContent {
    CompanionContent {
        running: true,
        shared: shared.then(project),
        documents: Ok(if shared { documents() } else { Vec::new() }),
        reply: None,
        review: None,
        rows: if rows {
            shelf::documents(false).0
        } else {
            Vec::new()
        },
        drag_hint: false,
    }
}

/// Capture `name` with the companion showing `content` on the stage; with
/// `hover`, the pointer rests on the reply.
fn stage(d: &mut Driver, name: &str, content: CompanionContent, hover: bool) -> Result<()> {
    let workspace = d
        .window
        .downcast::<Workspace>()
        .ok_or("the workspace window")?;
    let theme = d.change(|w, _, _| w.theme_cell())?;
    let height = crate::companion::height(&content);
    let window =
        d.cx.open_window(size(px(WIDTH), px(HEIGHT)), |window, cx| {
            let companion = cx.new(|cx| {
                let mut view = CompanionView::new(workspace, content, window, cx);
                view.freeze();
                view
            });
            cx.new(|_| Stage {
                companion,
                theme,
                height,
            })
        })
        .map_err(|e| e.to_string())?;
    let workspace_window = d.window;
    d.window = window.into();
    let result = hover_reply(d, window, hover).and_then(|()| d.capture(name));
    d.cx.update_window(d.window, |_, window, _| window.remove_window())
        .map_err(|e| e.to_string())?;
    d.window = workspace_window;
    result
}

/// Rest the pointer on the reply until its restored text shows.
fn hover_reply(d: &mut Driver, window: gpui::WindowHandle<Stage>, hover: bool) -> Result<()> {
    if !hover {
        return Ok(());
    }
    // Paint once, so the reply's place is known.
    d.settle()?;
    window
        .update(&mut d.cx, |stage, window, cx| {
            let center = (stage.companion.read(cx).reply_center()).ok_or("the reply")?;
            window.simulate_mouse_move(center, cx);
            Ok::<_, String>(())
        })
        .map_err(|e| e.to_string())??;
    d.cx.advance_clock(Duration::from_millis(100));
    let shown = window
        .update(&mut d.cx, |stage, _, cx| {
            stage.companion.read(cx).reply_shown()
        })
        .map_err(|e| e.to_string())?;
    if !shown {
        return Err("the reply did not show on hover".into());
    }
    Ok(())
}

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    let mut nothing = content(false, true);
    nothing.reply = Some(reply());
    stage(d, "companion-nothing-shared", nothing, false)?;
    stage(d, "companion-shared", content(true, true), false)?;
    let mut replied = content(true, false);
    replied.reply = Some(reply());
    stage(d, "companion-reply-hidden", replied.clone(), false)?;
    stage(d, "companion-reply-shown", replied, true)?;
    let mut review = content(true, false);
    review.review = Some(CompanionReview {
        document: format!("{:032x}", 0xd2),
        from: "Claude".into(),
        label: "Document 3".into(),
    });
    stage(d, "companion-review", review, false)?;
    stage(d, "companion-empty", content(false, false), false)
}
