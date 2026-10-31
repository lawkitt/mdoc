//! The menu bar shelf (ADR 0084): ready, not-ready and changed-since-review
//! rows with prose previews, case labels and the drag hint; a row's
//! "Copied"; the first-close note; the empty shelf and the locked shelf.
//! The shelf is drawn on a stage window of the usual size, at the top right
//! as it sits under the menu bar item; no status item or native popup (nor
//! its blur) is involved.
use super::{Driver, HEIGHT, Result, WIDTH};
use crate::{
    shelf::{ClosedMember, OpenTab, ShelfDocument, fingerprint},
    shelf_ui::{NOTE_HEIGHT, NOTE_WIDTH, NoteView, ShelfContent, ShelfView},
    style::Theme,
    workspace::{ClipboardSource, Workspace},
};
use gpui::{AnyView, App, Entity, Render, Window, div, prelude::*, px, size};
use mdoc_pii::{Category, restore::ExportedIdentity};
use mdoc_store::cases::{Case, CaseLink};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    path::Path,
    rc::Rc,
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
        let content = shelf.content();
        json!({
            "shelf": true,
            "rows": content.documents.iter().map(|d| json!({
                "title": d.display_title(),
                "state": d.readiness.label(),
                "preview": d.preview.as_ref().map(|p| p.text.clone()),
                "replaced": d.replaced_count(),
            })).collect::<Vec<_>>(),
            "copied": shelf.copied(),
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

/// Ready, not-ready and changed-since-review documents of one public,
/// synthetic case. Only its pseudonymized text ever reaches a row.
fn documents(locked: bool) -> (Vec<ShelfDocument>, ClipboardSource) {
    let names = vec![
        identity("PERSON_1", Category::Person, "Anna Petrova"),
        identity("ORG_1", Category::Organization, "Northwind Traders Ltd"),
        identity("PERSON_2", Category::Person, "Boris Ivanov"),
    ];
    let member = Path::new("/Cases/Anna Petrova notice.md");
    let reviewed = "PERSON_1 gives notice to ORG_1 under clause 9.\n";
    let mut case = Case::new("Petrova v Northwind".into(), &names, Some(member.into()));
    case.set_reviewed(member, Some(fingerprint(reviewed)));
    let link = CaseLink::of(&case);
    // Page furniture and a heading come first; the preview starts at the
    // prose, with its Markdown hidden.
    let ready = "— 1 —\n\n# Services Agreement\n\nPage 1 of 5\n\nThis **agreement** between \
                 PERSON_1 and ORG_1 sets out the services, fees and term of the engagement, \
                 and the _notice_ either may give.\n\nPERSON_2 signs for ORG_1.\n";
    let uncased = "Minutes of the meeting held with PERSON_1 on the revised payment schedule.\n";
    let pending = "PERSON_1 asked Boris Ivanov to review the draft.\n";
    let mut documents = vec![
        ShelfDocument::of_tab(OpenTab {
            id: 1,
            title: "Anna Petrova services agreement.md",
            text: ready,
            aliases: vec!["PERSON_1", "ORG_1", "PERSON_2"],
            pending: 0,
            checking: false,
            names: names.clone(),
            case: Some(link.clone()),
            edited: at(14),
        }),
        ShelfDocument::of_tab(OpenTab {
            id: 3,
            title: "Meeting notes 12 October.md",
            text: uncased,
            aliases: vec!["PERSON_1"],
            pending: 0,
            checking: false,
            names: names.clone(),
            case: None,
            edited: at(13),
        }),
        ShelfDocument::of_tab(OpenTab {
            id: 2,
            title: "Draft memo.md",
            text: pending,
            aliases: vec!["PERSON_1"],
            pending: 2,
            checking: false,
            names: names.clone(),
            case: Some(link),
            edited: at(12),
        }),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    // Cases locked: closed members are hidden.
    if !locked {
        documents.push(ShelfDocument::of_member(ClosedMember {
            case: &case,
            path: member,
            text: Some("PERSON_1 gives notice to ORG_1 under clause 9 and 10.\n"),
            edited: at(9),
        }));
    }
    let source = ClipboardSource::Case {
        id: case.id.clone(),
        name: case.name.clone(),
    };
    (documents, source)
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
    popup: impl FnOnce(gpui::WindowHandle<Workspace>, Rc<Cell<Theme>>, &mut Window, &mut App) -> (AnyView, Option<Entity<ShelfView>>)
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
    let result = d.capture(name);
    d.cx.update_window(d.window, |_, window, _| window.remove_window())
        .map_err(|e| e.to_string())?;
    d.window = workspace_window;
    result
}

fn shelf(d: &mut Driver, name: &str, content: ShelfContent, copied: Option<usize>) -> Result<()> {
    let height = content.height();
    stage(
        d,
        name,
        (crate::menu_bar::SHELF_WIDTH, height),
        move |workspace, _, window, cx| {
            let shelf = cx.new(|cx| {
                let mut view = ShelfView::new(workspace, content, window, cx);
                if let Some(row) = copied {
                    view.show_copied(row);
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
    let (rows, _) = documents(false);
    let mut copied = content(rows, source.clone());
    copied.drag_hint = false;
    shelf(d, "shelf-copied", copied, Some(0))?;
    stage(
        d,
        "shelf-note",
        (NOTE_WIDTH, NOTE_HEIGHT),
        |workspace, theme, window, cx| {
            let note = cx.new(|cx| NoteView::new(workspace, theme, window, cx));
            (note.into(), None)
        },
    )?;
    shelf(d, "shelf-empty", content(Vec::new(), source), None)?;
    // Locked as on a launch whose quiet probe could not read the key: open
    // tabs still show; the clipboard buttons ask to unlock.
    d.change(|w, _, _| {
        w.case_coordinator.reopen(None);
        w.case_coordinator.lock();
    })?;
    let (rows, source) = documents(true);
    shelf(d, "shelf-locked", content(rows, source), None)
}
