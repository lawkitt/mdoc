//! The menu bar shelf: ready, not-ready and changed-since-review
//! rows with prose previews, Project labels and the drag hint; the applied
//! demos and their "N replaced" hovered (#304); a row's "Copied"; the
//! first-close note; the empty shelf and the locked shelf; every row that
//! is not ready with its step, and a Copy for AI refused on one (#1209).
//! The shelf is drawn on a stage window of the usual size, at the top right
//! as it sits under the menu bar item; no status item or native popup (nor
//! its blur) is involved.
use super::{Driver, HEIGHT, Result, WIDTH};
use crate::{
    shelf::{OpenTab, ShelfDocument, StoredDocument},
    shelf_ui::{NOTE_HEIGHT, NOTE_WIDTH, NoteView, ShelfContent, ShelfView},
    style::Theme,
    workspace::{ClipboardSource, ProjectLink, Workspace},
};
use gpui::{AnyView, App, Entity, Render, Window, div, prelude::*, px, size};
use mdoc_pii::{Category, restore::ExportedIdentity};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, SystemTime},
};

/// The popup as it sits on screen, for renders only.
pub(super) struct Stage {
    popup: AnyView,
    shelf: Option<Entity<ShelfView>>,
    theme: Rc<Cell<Theme>>,
    width: f32,
    height: f32,
}

impl Stage {
    pub(super) fn state(&self, cx: &App) -> Value {
        let Some(shelf) = &self.shelf else {
            return json!({
                "shelf_note": true,
                "title": crate::shelf_ui::NOTE_TITLE,
                "text": crate::shelf_ui::NOTE_TEXT,
            });
        };
        let shelf = shelf.read(cx);
        let content = shelf.content(cx);
        json!({
            "shelf": true,
            "rows": content.documents.iter().map(|d| json!({
                "title": d.display_title(),
                "state": d.state(),
                "step": d.step().map(crate::refusal::Step::label),
                "preview": d.preview.as_ref().map(|p| p.text.clone()),
                "replaced": d.replaced_count(),
                "names": d.names_count(),
            })).collect::<Vec<_>>(),
            "copied": shelf.copied(cx),
            "refused": shelf.refused(cx),
            "drag_hint": content.drag_hint,
            "source_line": shelf.source_line(),
            "clipboard_locked": shelf.clipboard_locked(cx),
        })
    }
}

impl Render for Stage {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = self.theme.get();
        let p = theme.pdf_style();
        // The window's corners and shadow are the system's on screen; an
        // opaque ground stands in for the blur behind the shelf.
        div().size_full().bg(theme.sidebar_bg()).child(
            div()
                .absolute()
                .top(px(6.))
                .right(px(160.))
                .w(px(self.width))
                .h(px(self.height))
                .rounded(px(12.))
                .border_1()
                .border_color(p.border)
                .shadow_lg()
                .overflow_hidden()
                .bg(p.bg)
                .child(self.popup.clone()),
        )
    }
}

fn identity(alias: &str, category: Category, original: &str) -> ExportedIdentity {
    ExportedIdentity {
        alias: alias.into(),
        previous: Vec::new(),
        category,
        custom: false,
        originals: vec![original.into()],
    }
}

/// 12 October 2026 plus `hours`: a fixed past day, so rows read the same.
fn at(hours: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_763_200 + hours * 3600)
}

/// Ready, not-ready and changed-since-review documents, three of them in
/// one public, synthetic Project. Only their pseudonymized text ever
/// reaches a row.
fn documents(locked: bool) -> (Vec<ShelfDocument>, ClipboardSource) {
    let names = vec![
        identity("PERSON_1", Category::Person, "Anna Petrova"),
        identity("ORG_1", Category::Organization, "Northwind Traders Ltd"),
        identity("PERSON_2", Category::Person, "Boris Ivanov"),
    ];
    let link = ProjectLink {
        id: "c".repeat(32),
        name: "Petrova v Northwind".into(),
        colour: None,
    };
    // Page furniture and a heading come first; the preview starts at the
    // prose, with its Markdown hidden.
    let ready = "— 1 —\n\n# Services Agreement\n\nPage 1 of 5\n\nThis **agreement** between \
                 PERSON_1 and ORG_1 sets out the services, fees and term of the engagement, \
                 and the _notice_ either may give.\n\nPERSON_2 signs for ORG_1.\n";
    let uncased = "Minutes of the meeting held with PERSON_1 on the revised payment schedule.\n";
    let pending = "PERSON_1 asked Boris Ivanov to review the draft.\n";
    let mut documents = vec![
        ShelfDocument::of_tab(
            OpenTab {
                id: 1,
                title: "Anna Petrova services agreement.md",
                text: ready,
                aliases: vec!["PERSON_1", "ORG_1", "PERSON_2"],
                pending: 0,
                checking: false,
                names: names.clone(),
                project: Some(link.clone()),
                edited: at(14),
                stored: None,
            }
            .checked(),
        ),
        ShelfDocument::of_tab(
            OpenTab {
                id: 3,
                title: "Meeting notes 12 October.md",
                text: uncased,
                aliases: vec!["PERSON_1"],
                pending: 0,
                checking: false,
                names: names.clone(),
                project: None,
                edited: at(13),
                stored: None,
            }
            .checked(),
        ),
        ShelfDocument::of_tab(
            OpenTab {
                id: 2,
                title: "Draft memo.md",
                text: pending,
                aliases: vec!["PERSON_1"],
                pending: 2,
                checking: false,
                names: names.clone(),
                project: Some(link.clone()),
                edited: at(12),
                stored: None,
            }
            .checked(),
        ),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    // The Vault locked: documents no tab shows are not listed.
    if !locked && let Some(id) = mdoc_store::document_id::VaultId::parse(&"d".repeat(32)) {
        documents.extend(ShelfDocument::of_stored(StoredDocument {
            id,
            title: "Anna Petrova notice",
            text: "PERSON_1 gives notice to ORG_1 under clause 9 and 10.\n",
            names: names.clone(),
            project: Some(link.clone()),
            edited: at(9),
            scanned: true,
            outbound: Some(Err(mdoc_vault::Readiness::ChangedSinceReview)),
        }));
    }
    let source = ClipboardSource::Project {
        id: link.id.clone(),
        name: link.name.clone(),
    };
    (documents, source)
}

/// One row in each state that is not ready, each with its step: names
/// left to review in an open tab, and stored documents holding a known name,
/// changed since review, and never saved ready, scanned or not.
fn refusals() -> (Vec<ShelfDocument>, ClipboardSource) {
    let names = vec![
        identity("PERSON_1", Category::Person, "Anna Petrova"),
        identity("PERSON_2", Category::Person, "Boris Ivanov"),
    ];
    let link = ProjectLink {
        id: "c".repeat(32),
        name: "Petrova v Northwind".into(),
        colour: None,
    };
    let mut documents: Vec<ShelfDocument> = ShelfDocument::of_tab(OpenTab {
        id: 2,
        title: "Draft memo.md",
        text: "PERSON_1 asked Boris Ivanov to review the draft.
",
        aliases: vec!["PERSON_1"],
        pending: 2,
        checking: false,
        names: names.clone(),
        project: Some(link.clone()),
        edited: at(14),
        stored: None,
    })
    .into_iter()
    .collect();
    let stored = [
        (
            "Witness statement",
            mdoc_vault::Readiness::NamesNotReplaced(1),
            true,
        ),
        (
            "Anna Petrova notice",
            mdoc_vault::Readiness::ChangedSinceReview,
            true,
        ),
        ("Lease schedule", mdoc_vault::Readiness::NotReviewed, true),
        ("Reply notes", mdoc_vault::Readiness::NotReviewed, false),
    ];
    for (n, (title, readiness, scanned)) in stored.into_iter().enumerate() {
        let Some(id) = mdoc_store::document_id::VaultId::parse(&format!("{:032x}", 0xd0 + n))
        else {
            continue;
        };
        documents.extend(ShelfDocument::of_stored(StoredDocument {
            id,
            title,
            text: "PERSON_1 gives notice to Boris Ivanov under clause 9.
",
            names: names.clone(),
            project: Some(link.clone()),
            edited: at(12 - n as u64),
            scanned,
            outbound: Some(Err(readiness)),
        }));
    }
    let source = ClipboardSource::Project {
        id: link.id.clone(),
        name: link.name.clone(),
    };
    (documents, source)
}

/// The settlement demos as applied: the text their Copy for AI sent, without
/// its note, and the mapping it was sent with (`tests/fixtures/restore`).
fn demos() -> Result<Vec<ShelfDocument>> {
    let demos = [
        (
            "Settlement agreement.md",
            include_str!("../../../tests/fixtures/restore/claude-en/sent.md"),
            include_str!("../../../tests/fixtures/restore/claude-en/identities.json"),
        ),
        (
            "Соглашение об урегулировании.md",
            include_str!("../../../tests/fixtures/restore/claude-ru/sent.md"),
            include_str!("../../../tests/fixtures/restore/claude-ru/identities.json"),
        ),
    ];
    let mut documents = Vec::new();
    for (index, (title, sent, mapping)) in demos.into_iter().enumerate() {
        let text = sent
            .split_once("\n---\n")
            .map(|(_, text)| text.trim_start())
            .ok_or("demo note")?;
        let mapping: Vec<Value> = serde_json::from_str(mapping).map_err(|e| e.to_string())?;
        let names = mapping
            .iter()
            .map(|identity| {
                Ok(ExportedIdentity {
                    alias: identity["alias"].as_str().ok_or("demo alias")?.into(),
                    previous: Vec::new(),
                    custom: false,
                    category: Category::ALL
                        .into_iter()
                        .find(|c| Some(c.token()) == identity["category"].as_str())
                        .ok_or("demo category")?,
                    originals: identity["originals"]
                        .as_array()
                        .ok_or("demo originals")?
                        .iter()
                        .map(|v| v.as_str().map(Arc::from).ok_or("demo original"))
                        .collect::<std::result::Result<_, _>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let aliases = names.iter().map(|n| n.alias.as_str()).collect();
        documents.extend(ShelfDocument::of_tab(
            OpenTab {
                id: 10 + index as u64,
                title,
                text,
                aliases,
                pending: 0,
                checking: false,
                names: names.clone(),
                project: None,
                edited: at(14 - index as u64),
                stored: None,
            }
            .checked(),
        ));
    }
    Ok(documents)
}

fn content(documents: Vec<ShelfDocument>, source: ClipboardSource) -> ShelfContent {
    ShelfContent {
        documents,
        sources: vec![source.clone()],
        source: Some(source),
        update: None,
        drag_hint: true,
    }
}

/// Capture `name` with a popup, built in its own window, on the stage.
fn stage(
    d: &mut Driver,
    name: &str,
    (width, height): (f32, f32),
    hover: Option<usize>,
    popup: impl FnOnce(
        gpui::WindowHandle<Workspace>,
        Rc<Cell<Theme>>,
        &mut Window,
        &mut App,
    ) -> (AnyView, Option<Entity<ShelfView>>)
    + 'static,
) -> Result<()> {
    let workspace = d
        .window
        .downcast::<Workspace>()
        .ok_or("the workspace window")?;
    let theme = d.change(|w, _, _| w.theme_cell())?;
    let window =
        d.cx.open_window(size(px(WIDTH), px(HEIGHT)), |window, cx| {
            let (popup, shelf) = popup(workspace, theme.clone(), window, cx);
            cx.new(|_| Stage {
                popup,
                shelf,
                theme,
                width,
                height,
            })
        })
        .map_err(|e| e.to_string())?;
    let workspace_window = d.window;
    d.window = window.into();
    let result = hover_count(d, window, hover).and_then(|()| d.capture(name));
    d.cx.update_window(d.window, |_, window, _| window.remove_window())
        .map_err(|e| e.to_string())?;
    d.window = workspace_window;
    result
}

/// Rest the pointer on row `row`'s "N replaced" until its breakdown shows.
fn hover_count(
    d: &mut Driver,
    window: gpui::WindowHandle<Stage>,
    row: Option<usize>,
) -> Result<()> {
    let Some(row) = row else {
        return Ok(());
    };
    // Paint once, so the count's place is known.
    d.settle()?;
    window
        .update(&mut d.cx, |stage, window, cx| {
            let center = stage
                .shelf
                .as_ref()
                .and_then(|shelf| shelf.read(cx).count_center(row, cx))
                .ok_or("the row's count")?;
            window.simulate_mouse_move(center, cx);
            Ok::<_, String>(())
        })
        .map_err(|e| e.to_string())??;
    d.cx.advance_clock(Duration::from_millis(600));
    Ok(())
}

fn shelf(d: &mut Driver, name: &str, content: ShelfContent, copied: Option<usize>) -> Result<()> {
    shelf_hovering(d, name, content, copied, None)
}

/// The shelf with Copy for AI refused on row `row`, as ⌘C on it does.
fn shelf_refused(d: &mut Driver, name: &str, content: ShelfContent, row: usize) -> Result<()> {
    let height = content.height();
    stage(
        d,
        name,
        (crate::menu_bar::SHELF_WIDTH, height),
        None,
        move |workspace, _, window, cx| {
            let shelf = cx.new(|cx| ShelfView::new(workspace, content, window, cx));
            shelf.update(cx, |view, cx| view.copy(row, window, cx));
            (shelf.clone().into(), Some(shelf))
        },
    )
}

fn shelf_hovering(
    d: &mut Driver,
    name: &str,
    content: ShelfContent,
    copied: Option<usize>,
    hover: Option<usize>,
) -> Result<()> {
    let height = content.height();
    stage(
        d,
        name,
        (crate::menu_bar::SHELF_WIDTH, height),
        hover,
        move |workspace, _, window, cx| {
            let shelf = cx.new(|cx| {
                let mut view = ShelfView::new(workspace, content, window, cx);
                if let Some(row) = copied {
                    view.show_copied(row, cx);
                }
                view
            });
            (shelf.clone().into(), Some(shelf))
        },
    )
}

pub(super) fn scenes(d: &mut Driver) -> Result<()> {
    let (rows, source) = documents(false);
    shelf(d, "shelf", content(rows, source.clone()), None)?;
    let (rows, refusal_source) = refusals();
    let mut not_ready = content(rows, refusal_source.clone());
    not_ready.drag_hint = false;
    shelf(d, "shelf-refusals", not_ready, None)?;
    let (rows, _) = refusals();
    let mut refused = content(rows, refusal_source);
    refused.drag_hint = false;
    shelf_refused(d, "shelf-refused", refused, 0)?;
    for (name, hover) in [("shelf-demos", None), ("shelf-demos-count", Some(0))] {
        let mut demos = content(demos()?, source.clone());
        demos.drag_hint = false;
        shelf_hovering(d, name, demos, None, hover)?;
    }
    let (rows, _) = documents(false);
    let mut copied = content(rows, source.clone());
    copied.drag_hint = false;
    shelf(d, "shelf-copied", copied, Some(0))?;
    stage(
        d,
        "shelf-note",
        (NOTE_WIDTH, NOTE_HEIGHT),
        None,
        |workspace, theme, window, cx| {
            let note = cx.new(|cx| NoteView::new(workspace, theme, window, cx));
            (note.into(), None)
        },
    )?;
    shelf(d, "shelf-empty", content(Vec::new(), source), None)?;
    // Locked as on a launch whose quiet probe could not read the key: open
    // tabs still show; the clipboard buttons ask to unlock.
    d.change(|w, _, _| {
        w.vault.opened(super::super::vault_state::Opening::MayAsk);
        w.projects.clear();
    })?;
    let (rows, source) = documents(true);
    shelf(d, "shelf-locked", content(rows, source), None)
}
