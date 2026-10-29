//! What you did (ADR 0079): every decision kind is listed on its undo point,
//! travel equals ⌘Z / ⇧⌘Z presses, and text edits are not listed.
use super::*;
use crate::document_view::DocumentView;
use crate::pii::ui::PiiAddCandidate;
use gpui::{Context, Focusable};
use mdoc_pii::{Category, decisions::EntryState};

const SOURCE: &str = "Maya Bennett met Ms Bennett. Acme Ltd paid 555-0100. Maya Bennett left.";

fn seed(app: &mut DocumentView, cx: &mut Context<DocumentView>) {
    app.editor.update(cx, |e, cx| e.set_text(SOURCE, cx));
    app.pii.review = Default::default();
    app.pii.reviewing = true;
    app.pii.set_decision_clock(|| "14:02".into());
    let mut detections = Vec::new();
    for (value, category) in [
        ("Maya Bennett", Category::Person),
        ("Ms Bennett", Category::Person),
        ("Acme Ltd", Category::Organization),
    ] {
        for (at, _) in SOURCE.match_indices(value) {
            detections.push(mdoc_pii::Detection {
                range: at..at + value.len(),
                category,
                score: 0.9,
                recognizer: mdoc_pii::Recognizer::Model,
            });
        }
    }
    app.pii.review.ingest(SOURCE, detections).unwrap();
    app.pii.mapping.begin_review();
    app.review_changed(cx);
}

fn identity(app: &DocumentView, original: &str) -> u64 {
    let review = &app.pii.review;
    let variant = review
        .variants()
        .iter()
        .find(|v| v.original.as_ref() == original)
        .unwrap();
    review.variant_identity(variant.id).unwrap()
}

fn lines(app: &DocumentView, cx: &gpui::App) -> Vec<(String, EntryState)> {
    let path = app.editor.read(cx).history_path();
    app.pii
        .decisions()
        .rows(&path)
        .map(|(entry, state)| (entry.line(), state))
        .collect()
}

fn labels(app: &DocumentView, cx: &gpui::App) -> Vec<String> {
    lines(app, cx)
        .into_iter()
        .map(|(line, _)| line.trim_end_matches(" · 14:02").to_owned())
        .collect()
}

/// A plain text edit, as typing makes: one undo step, never listed.
fn type_text(app: &mut DocumentView, at: usize, text: &str, cx: &mut Context<DocumentView>) {
    app.editor
        .update(cx, |e, cx| e.replace_range(at..at, text, cx));
}

#[gpui::test]
fn every_decision_kind_is_listed_once_and_text_edits_are_not(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, seed);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let maya = identity(app, "Maya Bennett");
        app.change_mapping(MappingAction::Rename(maya, "CLIENT".into()), cx);
        let acme = identity(app, "Acme Ltd");
        app.change_mapping(MappingAction::Category(acme, Category::Person), cx);
        // Setting an owner and typing are undo steps, not decisions.
        app.change_mapping(MappingAction::Owner(acme, Some(maya)), cx);
        type_text(app, SOURCE.len(), " Signed.", cx);
        let ms = identity(app, "Ms Bennett");
        app.change_mapping(MappingAction::Merge(ms, maya), cx);
        let last = app.pii.review.variants()[0]
            .mentions
            .last()
            .cloned()
            .unwrap();
        app.change_mapping(
            MappingAction::Scoped {
                identity: maya,
                ranges: vec![last],
                target: None,
                alias: None,
                category: None,
            },
            cx,
        );
        let phone = SOURCE.find("555-0100").unwrap();
        app.editor
            .update(cx, |e, cx| e.set_selection(phone..phone + 8, cx));
        app.add_pii_candidate(&PiiAddCandidate, window, cx);
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.close_pii_popup(&super::super::PiiClosePopup, window, cx);
        app.apply_replacements(cx);
        let first = app.pii.review.applied()[0].id;
        app.undo_replacements([first].into(), cx);
        let candidate = app.pii.review.candidates()[0].clone();
        app.keep_mentions_now(vec![(candidate.variant, candidate.range)], vec![], cx);
        assert_eq!(
            labels(app, cx),
            [
                "Renamed “Maya Bennett” to CLIENT",
                "Changed “Acme Ltd” to Person",
                "Merged “Ms Bennett” into “Maya Bennett”",
                "Separated “Maya Bennett”",
                "Added “555-0100”",
                "Applied 5 replacements",
                "Undid replacement of “Maya Bennett”",
                "Kept “Maya Bennett”",
            ]
        );
        let (line, state) = lines(app, cx).pop().unwrap();
        assert_eq!(line, "Kept “Maya Bennett” · 14:02");
        assert_eq!(state, EntryState::Done);
    });
}

/// Run a decision, returning the editor's text and history state after it.
fn after(
    app: &Entity<DocumentView>,
    cx: &mut gpui::VisualTestContext,
    decide: impl FnOnce(&mut DocumentView, &mut Window, &mut Context<DocumentView>),
) -> (String, u64) {
    app.update_in(cx, |app, window, cx| {
        decide(app, window, cx);
        let editor = app.editor.read(cx);
        (editor.text().to_owned(), editor.history_id())
    })
}

fn state(app: &Entity<DocumentView>, cx: &mut gpui::VisualTestContext) -> (String, u64) {
    app.update(cx, |app, cx| {
        let editor = app.editor.read(cx);
        (editor.text().to_owned(), editor.history_id())
    })
}

fn travel(app: &Entity<DocumentView>, cx: &mut gpui::VisualTestContext, index: usize) {
    app.update_in(cx, |app, window, cx| {
        app.travel_to_decision(index, window, cx)
    });
    cx.run_until_parked();
}

fn press(cx: &mut gpui::VisualTestContext, app: &Entity<DocumentView>, undo: bool, times: usize) {
    for _ in 0..times {
        app.update_in(cx, |app, window, cx| {
            window.focus(&app.editor.read(cx).focus_handle(cx), cx)
        });
        if undo {
            cx.dispatch_action(mdoc_editor::Undo);
        } else {
            cx.dispatch_action(mdoc_editor::Redo);
        }
        cx.run_until_parked();
    }
}

#[gpui::test]
fn travel_equals_undo_and_redo_presses_and_a_new_decision_drops_undone_entries(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, seed);
    cx.run_until_parked();
    let renamed = after(&app, cx, |app, _, cx| {
        let maya = identity(app, "Maya Bennett");
        app.change_mapping(MappingAction::Rename(maya, "CLIENT".into()), cx);
    });
    // A text edit between decisions: undone with them, never listed.
    let typed = after(&app, cx, |app, _, cx| type_text(app, 0, "Note: ", cx));
    let applied = after(&app, cx, |app, _, cx| app.apply_replacements(cx));
    let kept = after(&app, cx, |app, _, cx| {
        let id = app.pii.review.applied()[0].id;
        app.keep_mentions_now(vec![], vec![id], cx);
    });
    app.update(cx, |app, cx| {
        assert_eq!(
            labels(app, cx),
            [
                "Renamed “Maya Bennett” to CLIENT",
                "Applied 4 replacements",
                "Kept “Maya Bennett”"
            ]
        );
    });

    // Back to just after the rename: three ⌘Z, the typing among them.
    travel(&app, cx, 0);
    assert_eq!(state(&app, cx), renamed);
    app.update(cx, |app, cx| {
        let states: Vec<_> = lines(app, cx).into_iter().map(|(_, s)| s).collect();
        assert_eq!(
            states,
            [EntryState::Done, EntryState::Undone, EntryState::Undone]
        );
    });
    // The same three presses from the end land on the same state.
    travel(&app, cx, 2);
    assert_eq!(state(&app, cx), kept);
    press(cx, &app, true, 3);
    assert_eq!(state(&app, cx), renamed);
    press(cx, &app, false, 1);
    assert_eq!(state(&app, cx), typed);
    // Clicking an undone entry redoes up to it: one ⇧⌘Z here.
    travel(&app, cx, 1);
    assert_eq!(state(&app, cx), applied);
    press(cx, &app, true, 1);
    press(cx, &app, false, 1);
    assert_eq!(state(&app, cx), applied);

    // A new decision after travelling back drops the undone entries.
    travel(&app, cx, 0);
    app.update(cx, |app, cx| {
        let acme = identity(app, "Acme Ltd");
        app.change_mapping(MappingAction::Category(acme, Category::Person), cx);
        assert_eq!(
            labels(app, cx),
            [
                "Renamed “Maya Bennett” to CLIENT",
                "Changed “Acme Ltd” to Person"
            ]
        );
        assert!(
            lines(app, cx)
                .iter()
                .all(|(_, state)| *state == EntryState::Done)
        );
    });
    // Typing after an undo drops them too, as it drops Redo.
    travel(&app, cx, 0);
    app.update(cx, |app, cx| {
        type_text(app, 0, "x", cx);
        assert_eq!(labels(app, cx), ["Renamed “Maya Bennett” to CLIENT"]);
    });
}
