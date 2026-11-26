//! Projects: the Open Project list, the move menu from the Share tab and
//! the ⋯ menu with no Project and one (#1312), a move into a Project with
//! its toast,
//! the Project page's three segments and its ⋯ menu,
//! the Project page with its rename, categories and lists, the Keychain
//! explainer, and the delete question.
use super::{Driver, Result};
use crate::workspace::project_page::{Place, Segment};

/// Moves the fixture document into the fixture Project; returns the
/// Project's id.
pub(super) fn scenes(d: &mut Driver) -> Result<String> {
    d.change(|w, window, cx| w.show_project_list(window, cx))?;
    d.capture("open-project")?;
    d.change(|w, window, cx| w.close_project_list(window, cx))?;
    let id = d.change(|w, _, _| {
        (w.projects.summaries().into_iter())
            .find(|summary| summary.name == "Public fixture")
            .map(|summary| summary.id)
    })?;
    let id = id.ok_or("fixture Project")?;
    move_menus(d)?;
    d.change(|w, window, cx| {
        let tab = w.tabs.active();
        w.move_tab(tab, Some(id.clone()), window, cx);
    })?;
    d.settle()?;
    d.capture("project-move-toast")?;
    d.change(|w, window, cx| {
        w.dismiss_toast(cx);
        // A closed member beside the open one, and a fixed creation date.
        let vault = w.vault.vault().cloned().expect("the fixture Vault");
        vault
            .create_document(mdoc_vault::NewDocument {
                project: Some(id.clone()),
                title: "Witness statement".into(),
                source_kind: mdoc_vault::SourceKind::Markdown,
                origin: mdoc_vault::DocumentOrigin::Ordinary,
                markdown: "# Public witness statement\n".into(),
            })
            .expect("temporary fixture document");
        w.projects.edit(&id, |project| {
            project.created = 1791763200;
            Some(())
        });
        w.open_project(&id, window, cx);
    })?;
    d.capture("project-page")?;
    // Its saved replies: one of the Project's, from ChatGPT.
    d.change(|w, _, cx| -> Result<()> {
        let vault = w.vault.vault().cloned().ok_or("fixture Vault")?;
        let alias = (w.projects.project(&id))
            .and_then(|project| project.mapping.exported().first().map(|i| i.alias.clone()))
            .ok_or("a fixture name")?;
        vault
            .save_reply(mdoc_vault::NewReply {
                owner: mdoc_vault::ReplyOwner::Project(id.clone()),
                title: "Reply to Public fixture".into(),
                source_app: Some("ChatGPT".into()),
                markdown: format!("{alias} keeps the deposit."),
            })
            .map_err(|e| e.to_string())?;
        w.show_project_segment(Segment::Replies, cx);
        Ok(())
    })??;
    d.capture("project-page-replies")?;
    d.change(|w, _, cx| w.show_project_segment(Segment::Names, cx))?;
    d.capture("project-page-names")?;
    // The header's ⋯ menu, opened under its button.
    d.change(|w, window, cx| {
        w.show_project_segment(Segment::Documents, cx);
        let width = window.viewport_size().width;
        let at = gpui::point(width / 2. + gpui::px(260.), gpui::px(118.));
        w.open_project_menu(&id, at, window, cx);
    })?;
    d.capture("project-page-menu")?;
    d.key("escape")?;
    d.change(|w, window, cx| w.start_rename_project(id.clone(), window, cx))?;
    d.capture("rename-project")?;
    d.key("escape")?;
    d.change(|w, window, cx| {
        w.show_project_page(&id, Place::Categories, window, cx);
    })?;
    d.capture("project-categories")?;
    // The Project's Always and Never lists, each entry with Remove.
    d.change(|w, window, cx| {
        w.projects.edit(&id, |project| {
            use mdoc_pii::Category;
            project
                .lists
                .add_always("Northwind Holdings", Category::Organization);
            project.lists.add_always("Maya Bennett", Category::Person);
            project.lists.add_never("High Court");
            project.lists.add_never("Riverside Clinic");
            Some(())
        });
        w.show_project_page(&id, Place::Categories, window, cx);
    })?;
    d.capture("project-page-lists")?;
    d.change(|w, _, _| {
        w.projects.edit(&id, |project| {
            project.lists = Default::default();
            Some(())
        });
    })?;
    d.change(|w, window, cx| w.activate(w.tabs.active(), window, cx))?;
    d.change(|w, _, cx| {
        // Preview only. Continue is never invoked and no OS key is read.
        w.project_ui.explainer = true;
        w.explainer_focus_pending = true;
        cx.notify();
    })?;
    d.capture("keychain-explainer")?;
    d.change(|w, window, cx| w.cancel_unlock(window, cx))?;
    Ok(id)
}

/// The Unfiled fixture document's Share tab and ⋯ menu with no Project
/// (the window's copy only, put back after) and with the fixture Project,
/// each with the move menu they open (#1312). The Project menu's Move to
/// Project… opens the same menu under ⋯.
fn move_menus(d: &mut Driver) -> Result<()> {
    let tab = d.change(|w, _, _| w.tabs.active())?;
    let view = d.change(|w, _, _| w.tab_view(tab))?;
    let view = view.ok_or("the fixture document's tab")?;
    let was = d.change(|_, _, cx| {
        let v = view.read(cx);
        (v.inspector, v.replacements_open())
    })?;
    let share = |d: &mut Driver, name: &str| -> Result<()> {
        d.change(|_, window, cx| {
            view.update(cx, |view, cx| {
                view.inspector = crate::document_view::Inspector::Share;
                if !view.replacements_open() {
                    view.toggle_replacements_panel(window, cx);
                }
                cx.notify();
            })
        })?;
        d.capture(name)
    };
    let menu = |d: &mut Driver, name: &str, under_more: bool| -> Result<()> {
        d.change(|w, window, cx| {
            let v = view.read(cx);
            let at = if under_more {
                v.more_bounds.get().bottom_left()
            } else {
                v.share_move_bounds.get().bottom_left()
            };
            w.open_move_menu(tab, at, window, cx);
        })?;
        d.capture(name)?;
        d.change(|w, window, cx| w.close_document_menu(window, cx))
    };
    // No Project.
    let kept = d.change(|w, _, cx| {
        let kept: Vec<_> = (w.projects.summaries().into_iter())
            .filter_map(|summary| w.projects.project(&summary.id).cloned())
            .collect();
        w.projects.finish_load(Vec::new());
        cx.notify();
        kept
    })?;
    share(d, "share-project-none")?;
    menu(d, "more-move-menu-none", true)?;
    d.change(|w, _, cx| {
        w.projects.finish_load(kept);
        cx.notify();
    })?;
    // One Project.
    share(d, "share-project-one")?;
    menu(d, "share-move-menu", false)?;
    d.change(|_, window, cx| {
        view.update(cx, |view, cx| {
            view.more_open = true;
            window.focus(&view.more_menu_focus, cx);
            cx.notify();
        })
    })?;
    d.capture("more-menu-move")?;
    d.change(|_, _, cx| {
        view.update(cx, |view, cx| {
            view.more_open = false;
            cx.notify();
        })
    })?;
    menu(d, "more-move-menu", true)?;
    d.change(|_, window, cx| {
        view.update(cx, |view, cx| {
            view.inspector = was.0;
            if view.replacements_open() != was.1 {
                view.toggle_replacements_panel(window, cx);
            }
            cx.notify();
        })
    })?;
    d.settle()
}

/// The delete question of a Project with documents, cancelled.
pub(super) fn delete(d: &mut Driver, id: String) -> Result<()> {
    d.change(|w, window, cx| w.delete_project(id, window, cx))?;
    d.capture("confirm-delete-project")?;
    d.key("escape")
}
