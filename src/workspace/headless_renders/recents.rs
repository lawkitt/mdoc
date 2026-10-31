//! Home › Recent after an unapplied scan and a saved conversion (#306): the
//! badge only on documents with applied replacements, and the converted
//! demo listed once, as its `.md`.
use super::{Driver, Result};
use crate::{document_view::DocumentView, pii::ui::PiiApplyAll};
use gpui::Entity;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const DEMO: &str = "Соглашение об урегулировании";

pub(super) fn scenes(d: &mut Driver, path: &Path) -> Result<()> {
    let folder = path.with_file_name("Matters");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    // Recent shows only this scene's files, not earlier scenes' (the DOCX
    // source scene previews the same demo from its own folder).
    d.change(|w, _, cx| w.clear_recents(cx))?;

    // A Markdown file pseudonymized with a proposal, nothing applied.
    let notes = folder.join("Meeting notes.md");
    std::fs::write(&notes, "Maya Bennett called about the delivery.\n")
        .map_err(|e| e.to_string())?;
    let view = open(d, &notes)?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            let text = v.editor.read(cx).text().to_string();
            let proposal = mdoc_pii::Detection {
                range: 0..12,
                category: mdoc_pii::Category::Person,
                score: 1.,
                recognizer: mdoc_pii::Recognizer::Model,
            };
            v.pii
                .review_mut()
                .ingest(&text, vec![proposal])
                .map_err(|e| format!("{e:?}"))?;
            v.review_changed(cx);
            v.toggle_replacements_panel(window, cx);
            assert!(v.pseudonymized() && !v.has_applied_aliases());
            Ok::<_, String>(())
        })
    })??;
    d.settle()?;

    // The Russian demo DOCX: converted, one name applied, saved as .md.
    let demo = folder.join(format!("{DEMO}.docx"));
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("docs/product-media/{DEMO}.docx")),
        &demo,
    )
    .map_err(|e| e.to_string())?;
    let view = open(d, &demo)?;
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| {
            let text = v.editor.read(cx).text().to_string();
            let word = text
                .split(|c: char| !c.is_alphabetic())
                .find(|word| word.chars().count() > 3)
                .ok_or("the demo converted to no words")?
                .to_string();
            let start = text.find(&word).expect("the word came from the text");
            v.pii
                .review_mut()
                .add_manual(&text, start..start + word.len(), mdoc_pii::Category::Person)
                .map_err(|e| format!("{e:?}"))?;
            v.review_changed(cx);
            v.toggle_replacements_panel(window, cx);
            v.apply_all_pii(&PiiApplyAll, window, cx);
            Ok::<_, String>(())
        })
    })??;
    d.settle()?;
    let saved = folder.join(format!("{DEMO}.md"));
    d.change(|_, window, cx| {
        view.update(cx, |v, cx| v.save_to(saved, None, window, cx));
    })?;
    d.settle()?;

    d.change(|w, window, cx| {
        let files = &w.shell.recents.files;
        let badged = |name: &str| {
            files
                .iter()
                .find(|r| r.path.ends_with(Path::new("Matters").join(name)))
                .map(|r| r.pseudonymized)
        };
        assert_eq!(files.len(), 2, "{files:?}");
        assert_eq!(badged("Meeting notes.md"), Some(false), "{files:?}");
        assert_eq!(badged(&format!("{DEMO}.md")), Some(true), "{files:?}");
        assert_eq!(badged(&format!("{DEMO}.docx")), None, "{files:?}");
        w.show_home(window, cx);
        window.focus(w.focus_regions.home(), cx);
    })?;
    d.capture("home-recents")
}

/// Drop `path` into a new tab and wait until its Markdown is ready.
fn open(d: &mut Driver, path: &Path) -> Result<Entity<DocumentView>> {
    d.change(|w, window, cx| {
        w.drop_files(
            &gpui::ExternalPaths(vec![path.to_path_buf()].into()),
            false,
            window,
            cx,
        );
    })?;
    // A conversion runs on a background thread; give it real time.
    for _ in 0..600 {
        d.settle()?;
        let ready = d.change(|w, _, cx| {
            w.active_view().filter(|view| {
                let view = view.read(cx);
                !view.lifecycle.loading()
                    && !view.lifecycle.source_only()
                    && !view.import.busy()
                    && !view.editor.read(cx).text().trim().is_empty()
            })
        })?;
        if let Some(view) = ready {
            return Ok(view);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("{} did not open", path.display()))
}
