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

fn history(app: &DocumentView, cx: &gpui::App) -> u64 {
    app.editor.read(cx).history_id()
}

fn mentions(app: &DocumentView, original: &str) -> Vec<Range<usize>> {
    let review = &app.pii.review;
    review
        .candidates()
        .iter()
        .filter(|c| review.variant(c.variant).unwrap().original.as_ref() == original)
        .map(|c| c.range.clone())
        .collect()
}

#[gpui::test]
fn a_rename_confirmed_with_apply_is_one_entry_naming_both(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, seed);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let maya = identity(app, "Maya Bennett");
        let before = history(app, cx);
        app.change_mapping_with_apply(
            MappingAction::Rename(maya, "CLIENT".into()),
            Applying::All,
            cx,
        );
        assert!(app.editor.read(cx).text().starts_with("CLIENT met PERSON_"));
        assert_eq!(
            labels(app, cx),
            ["Renamed “Maya Bennett” to CLIENT · applied 4 replacements"]
        );
        // One undo point.
        let path = app.editor.read(cx).history_path();
        assert_eq!(path.steps_to(before), Some(-1));
    });
}

#[gpui::test]
fn re_choosing_the_current_category_makes_no_step_and_no_entry(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, seed);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let acme = identity(app, "Acme Ltd");
        let before = history(app, cx);
        app.change_mapping(MappingAction::Category(acme, Category::Organization), cx);
        assert_eq!(history(app, cx), before);
        assert_eq!(app.editor.read(cx).text(), SOURCE);
        assert!(labels(app, cx).is_empty());
        assert!(app.pii.error.is_none());
    });
}

#[gpui::test]
fn re_ticking_a_kept_mention_is_listed(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, seed);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let maya = identity(app, "Maya Bennett");
        let candidate = app
            .pii
            .review
            .candidates()
            .iter()
            .find(|c| c.range.start == 0)
            .cloned()
            .unwrap();
        app.keep_mentions_now(
            vec![(candidate.variant, candidate.range.clone())],
            vec![],
            cx,
        );
        let kept = history(app, cx);
        app.retick_mention(maya, candidate.range, cx);
        assert_ne!(history(app, cx), kept);
        assert_eq!(
            labels(app, cx),
            ["Kept “Maya Bennett”", "Proposed “Maya Bennett” again"]
        );
    });
}

#[gpui::test]
fn mention_scoped_category_is_a_split_and_alias_a_rename(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, seed);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let maya = identity(app, "Maya Bennett");
        let ms = identity(app, "Ms Bennett");
        app.change_mapping(MappingAction::Merge(ms, maya), cx);
        let identities = app.pii.review.active_identities().len();
        let [first, second] = mentions(app, "Maya Bennett").try_into().unwrap();
        app.change_mapping(
            MappingAction::Scoped {
                identity: maya,
                ranges: vec![first],
                target: None,
                alias: None,
                category: Some(Category::Organization),
            },
            cx,
        );
        assert_eq!(app.pii.review.active_identities().len(), identities + 1);
        app.change_mapping(
            MappingAction::Scoped {
                identity: maya,
                ranges: vec![second],
                target: None,
                alias: Some("CLIENT".into()),
                category: None,
            },
            cx,
        );
        assert_eq!(
            labels(app, cx),
            [
                "Merged “Ms Bennett” into “Maya Bennett”",
                "Separated “Maya Bennett” as Organization",
                "Renamed “Maya Bennett” to CLIENT",
            ]
        );
    });
}

#[gpui::test]
fn apply_counts_alias_corrections_beside_proposals(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, seed);
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let maya = identity(app, "Maya Bennett");
        let acme = identity(app, "Acme Ltd");
        app.apply_replacements(cx);
        let acme_applied = app
            .pii
            .review
            .applied()
            .iter()
            .find(|a| a.step.identity == acme)
            .unwrap()
            .id;
        app.undo_replacements([acme_applied].into(), cx);
        // Maya's two applied mentions now need their alias corrected, and
        // Acme is proposed again: Apply makes three replacements.
        app.pii.review.rename_identity(maya, "CLIENT").unwrap();
        app.apply_replacements(cx);
        assert!(app.editor.read(cx).text().starts_with("CLIENT met "));
        assert_eq!(labels(app, cx).last().unwrap(), "Applied 3 replacements");
    });
}

#[gpui::test]
fn a_decision_past_the_undo_limit_stays_listed_and_disabled(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, seed);
    cx.run_until_parked();
    let decided = app.update(cx, |app, cx| {
        let maya = identity(app, "Maya Bennett");
        app.change_mapping(MappingAction::Rename(maya, "CLIENT".into()), cx);
        history(app, cx)
    });
    // Edits until the decision's point is the oldest Undo still reaches.
    for _ in 0..300 {
        let oldest = app.update(cx, |app, cx| app.editor.read(cx).history_path().undo[0]);
        if oldest == decided {
            break;
        }
        app.update(cx, |app, cx| type_text(app, 0, "x", cx));
    }
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let path = app.editor.read(cx).history_path();
        assert_eq!(path.undo[0], decided);
        assert_eq!(path.undo.len(), 256, "the editor's undo limit");
        assert_eq!(
            lines(app, cx),
            [(
                "Renamed “Maya Bennett” to CLIENT · 14:02".into(),
                EntryState::Done
            )]
        );
        assert_eq!(app.pii.decisions().steps_to(0, &path), Some(-256));
    });
    // One more edit evicts it: still listed, no longer reached.
    app.update(cx, |app, cx| type_text(app, 0, "x", cx));
    cx.run_until_parked();
    let now = app.update(cx, |app, cx| {
        let path = app.editor.read(cx).history_path();
        assert_eq!(path.steps_to(decided), None);
        assert_eq!(
            lines(app, cx),
            [(
                "Renamed “Maya Bennett” to CLIENT · 14:02".into(),
                EntryState::PastLimit
            )]
        );
        assert_eq!(app.pii.decisions().steps_to(0, &path), None);
        history(app, cx)
    });
    travel(&app, cx, 0);
    app.update(cx, |app, cx| assert_eq!(history(app, cx), now));
    // A later decision is listed after it as usual.
    app.update(cx, |app, cx| {
        let acme = identity(app, "Acme Ltd");
        app.change_mapping(MappingAction::Category(acme, Category::Person), cx);
        assert_eq!(
            lines(app, cx)
                .into_iter()
                .map(|(_, s)| s)
                .collect::<Vec<_>>(),
            [EntryState::PastLimit, EntryState::Done]
        );
    });
}

#[gpui::test]
fn undo_with_the_popover_open_redraws_it(cx: &mut gpui::TestAppContext) {
    let (app, cx) = crate::document_view::tests::boot(cx);
    app.update(cx, seed);
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        let maya = identity(app, "Maya Bennett");
        // A metadata-only step: the text does not change.
        app.change_mapping(MappingAction::Rename(maya, "CLIENT".into()), cx);
        app.open_decision_history(window, cx);
    });
    cx.run_until_parked();
    let notified = std::rc::Rc::new(std::cell::Cell::new(0));
    let _observe = cx.update(|_, cx| {
        let notified = notified.clone();
        cx.observe(&app, move |_, _| notified.set(notified.get() + 1))
    });
    press(cx, &app, true, 1);
    assert!(notified.get() > 0, "an open popover recomputes its rows");
    app.update(cx, |app, cx| {
        assert!(app.history.open);
        assert_eq!(lines(app, cx)[0].1, EntryState::Undone);
    });
}
