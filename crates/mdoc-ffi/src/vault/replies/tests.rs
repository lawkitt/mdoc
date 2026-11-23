//! AI Replies through the exported objects (#1091, Testing Decisions
//! seam 2: a reply saved through a second handle), on a temp folder with
//! an injected key. `ReplyVault` stands in for the Restore extension.
use super::*;
use crate::{
    document_session::{Candidate, Category, DocumentSession},
    library::TextRange,
    test_support::Scratch,
};

const KEY: [u8; 32] = [9; 32];

fn root(scratch: &Scratch) -> String {
    scratch.path().join("vault").display().to_string()
}

fn app(scratch: &Scratch) -> Arc<Vault> {
    Vault::open(root(scratch), KEY.to_vec()).unwrap()
}

fn extension(scratch: &Scratch) -> Arc<ReplyVault> {
    ReplyVault::open(root(scratch), KEY.to_vec()).unwrap()
}

/// A document with `names` replaced, saved into Project `project` or
/// Unfiled; its id.
fn saved(vault: &Arc<Vault>, project: Option<&str>, text: &str, names: &[&str]) -> String {
    let session = DocumentSession::new();
    session.import_text(text.into(), "Letter".into()).unwrap();
    if let Some(project) = project {
        session.join_project(vault.clone(), project.into()).unwrap();
    }
    let candidates = names
        .iter()
        .map(|wording| {
            let start = text.find(wording).unwrap() as u64;
            Candidate {
                range: TextRange {
                    start,
                    end: start + wording.len() as u64,
                },
                category: Category::Person,
                score: 0.9,
            }
        })
        .collect();
    session.add_candidates(candidates).unwrap();
    session.replace_all().unwrap();
    session.save_into_vault(vault.clone()).unwrap().id
}

fn ids(replies: &[ReplyInfo]) -> Vec<String> {
    replies.iter().map(|r| r.id.clone()).collect()
}

fn document(id: &str) -> SharedFrom {
    SharedFrom::Document { id: id.into() }
}

/// The item's Done-when: replies the Restore extension's handle saves, for
/// a document in a Project and for an Unfiled one, show in the app's lists
/// after `refresh()` and restore there; the extension never writes the
/// manifest.
#[test]
fn a_reply_saved_through_a_second_handle_shows_in_the_first_and_restores() {
    let scratch = Scratch::new();
    let app = app(&scratch);
    let ivanov = app.create_project("Ivanov".into()).unwrap();
    let letter = saved(
        &app,
        Some(&ivanov.id),
        "Anna Ivanova wrote.",
        &["Anna Ivanova"],
    );
    let memo = saved(&app, None, "Carl Fox wrote.", &["Carl Fox"]);
    let manifest = scratch.path().join("vault").join("manifest.enc");
    let before = std::fs::read(&manifest).unwrap();

    let extension = extension(&scratch);
    assert_eq!(
        ids_of(&extension.projects()),
        std::slice::from_ref(&ivanov.id)
    );
    assert_eq!(extension.documents_in(ivanov.id.clone())[0].id, letter);
    assert_eq!(extension.unfiled()[0].id, memo);
    let reply = "PERSON_1 agrees; PERSON_2 does not.";
    let preview = extension
        .restore_text(document(&letter), reply.into())
        .unwrap();
    assert_eq!(preview.text, "Anna Ivanova agrees; PERSON_2 does not.");
    assert_eq!((preview.values, preview.unrecognised), (1, 1));
    let for_letter = extension
        .save_reply(
            document(&letter),
            "From Claude".into(),
            Some("Claude".into()),
            reply.into(),
        )
        .unwrap();
    assert_eq!(
        for_letter.owner,
        ReplyOwner::Project {
            id: ivanov.id.clone()
        },
        "a reply to text from a document in a Project is the Project's"
    );
    let for_memo = extension
        .save_reply(document(&memo), "Memo reply".into(), None, reply.into())
        .unwrap();
    assert_eq!(
        for_memo.owner,
        ReplyOwner::Document { id: memo.clone() },
        "an Unfiled document's is the document's"
    );
    assert_eq!(
        ids(&extension.project_replies(ivanov.id.clone())),
        std::slice::from_ref(&for_letter.id)
    );
    assert_eq!(
        extension.restore_reply(for_letter.id.clone()).unwrap(),
        preview
    );
    drop(extension);
    assert_eq!(
        std::fs::read(&manifest).unwrap(),
        before,
        "the extension never writes the manifest"
    );

    assert!(app.project_replies(ivanov.id.clone()).is_empty());
    assert!(app.refresh().unwrap());
    assert_eq!(
        app.project_replies(ivanov.id.clone()),
        std::slice::from_ref(&for_letter)
    );
    assert_eq!(
        app.document_replies(memo.clone()),
        std::slice::from_ref(&for_memo)
    );
    assert_eq!(app.restore_reply(for_letter.id.clone()).unwrap(), preview);
    let restored = app.restore_reply(for_memo.id.clone()).unwrap();
    assert_eq!(restored.text, "Carl Fox agrees; PERSON_2 does not.");
    assert!(restored.restorable);
    let kept = app.reply(for_letter.id.clone()).unwrap();
    assert_eq!(kept.markdown, reply, "kept as received");
    assert_eq!(kept.info, for_letter);
    assert_eq!(kept.info.source_app.as_deref(), Some("Claude"));
}

fn ids_of(projects: &[ProjectInfo]) -> Vec<String> {
    projects.iter().map(|p| p.id.clone()).collect()
}

/// A reply saved from a Project's page is the Project's; an Unfiled
/// document's reply follows it into a Project and restores the same person
/// there, whose alias now means someone else in the Project.
#[test]
fn replies_belong_to_a_project_or_an_unfiled_document() {
    let scratch = Scratch::new();
    let app = app(&scratch);
    let ivanov = app.create_project("Ivanov".into()).unwrap();
    saved(
        &app,
        Some(&ivanov.id),
        "Anna Ivanova wrote.",
        &["Anna Ivanova"],
    );
    let memo = saved(&app, None, "Carl Fox wrote.", &["Carl Fox"]);
    let page = app
        .save_reply(
            SharedFrom::Project {
                id: ivanov.id.clone(),
            },
            "Page reply".into(),
            None,
            "PERSON_1 agrees.".into(),
        )
        .unwrap();
    assert_eq!(
        page.owner,
        ReplyOwner::Project {
            id: ivanov.id.clone()
        }
    );
    let for_memo = app
        .save_reply(
            document(&memo),
            "Memo reply".into(),
            None,
            "PERSON_1 agrees.".into(),
        )
        .unwrap();
    assert_eq!(
        ids(&app.document_replies(memo.clone())),
        std::slice::from_ref(&for_memo.id)
    );
    assert!(app.document_replies(ivanov.id.clone()).is_empty());

    app.move_document(memo.clone(), Some(ivanov.id.clone()))
        .unwrap();
    let mut listed = ids(&app.project_replies(ivanov.id.clone()));
    listed.sort();
    let mut expected = vec![page.id.clone(), for_memo.id.clone()];
    expected.sort();
    assert_eq!(listed, expected, "it followed the document");
    assert_eq!(
        ids(&app.document_replies(memo.clone())),
        std::slice::from_ref(&for_memo.id)
    );
    assert_eq!(
        app.restore_reply(for_memo.id.clone()).unwrap().text,
        "Carl Fox agrees."
    );
    assert_eq!(
        app.restore_reply(page.id.clone()).unwrap().text,
        "Anna Ivanova agrees."
    );
    // Shared from the moved document now, a reply is the Project's.
    let after = app
        .save_reply(
            document(&memo),
            "Later".into(),
            None,
            "PERSON_2 agrees.".into(),
        )
        .unwrap();
    assert_eq!(after.owner, ReplyOwner::Project { id: ivanov.id });
    assert_eq!(
        app.restore_reply(after.id).unwrap().text,
        "Carl Fox agrees."
    );
}

/// Deleting a reply removes it from both handles' lists; blank replies and
/// titles, unknown places and ids are refused.
#[test]
fn replies_are_deleted_and_bad_ones_refused() {
    let scratch = Scratch::new();
    let app = app(&scratch);
    let ivanov = app.create_project("Ivanov".into()).unwrap();
    let project = SharedFrom::Project {
        id: ivanov.id.clone(),
    };
    let reply = app
        .save_reply(
            project.clone(),
            "  Reply  ".into(),
            Some(" ".into()),
            "Hi.".into(),
        )
        .unwrap();
    assert_eq!(reply.title, "Reply");
    assert_eq!(reply.source_app, None);
    let extension = extension(&scratch);
    assert_eq!(
        ids(&extension.project_replies(ivanov.id.clone())),
        std::slice::from_ref(&reply.id)
    );
    app.delete_reply(reply.id.clone()).unwrap();
    assert!(app.project_replies(ivanov.id.clone()).is_empty());
    assert!(extension.refresh().unwrap());
    assert!(extension.project_replies(ivanov.id.clone()).is_empty());
    assert!(extension.reply(reply.id.clone()).is_err());

    for (title, text) in [(" ", "Hi."), ("Reply", " \n")] {
        assert!(matches!(
            app.save_reply(project.clone(), title.into(), None, text.into()),
            Err(CoreError::Invalid { .. })
        ));
    }
    let missing = "0".repeat(32);
    for from in [
        SharedFrom::Project {
            id: missing.clone(),
        },
        document(&missing),
        document("nope"),
    ] {
        assert!(
            extension
                .save_reply(from.clone(), "Reply".into(), None, "Hi.".into())
                .is_err()
        );
        assert!(app.restore_text(from, "Hi.".into()).is_err());
    }
    assert!(app.restore_reply(missing.clone()).is_err());
    assert!(app.delete_reply("nope".into()).is_err());
}

/// The extension's handle opens only a Vault the app made, with its key.
#[test]
fn the_extensions_handle_needs_the_vault_and_its_key() {
    let scratch = Scratch::new();
    assert!(ReplyVault::open(root(&scratch), KEY.to_vec()).is_err());
    assert!(!scratch.path().join("vault").exists(), "nothing made");
    let app = app(&scratch);
    app.create_project("Ivanov".into()).unwrap();
    drop(app);
    assert!(ReplyVault::open(root(&scratch), vec![8; 32]).is_err());
    assert!(ReplyVault::open(root(&scratch), vec![9; 16]).is_err());
    assert_eq!(extension(&scratch).projects().len(), 1);
}

/// Copy and Share follow the owner's names, as the library's restore does:
/// a reply that names none of them is still restorable while its Project
/// has names, and not in a Project with none.
#[test]
fn a_reply_is_restorable_while_its_owner_has_names() {
    let scratch = Scratch::new();
    let app = app(&scratch);
    let ivanov = app.create_project("Ivanov".into()).unwrap();
    saved(
        &app,
        Some(&ivanov.id),
        "Anna Ivanova wrote.",
        &["Anna Ivanova"],
    );
    let empty = app.create_project("Empty".into()).unwrap();
    for (project, restorable) in [(ivanov, true), (empty, false)] {
        let from = SharedFrom::Project { id: project.id };
        let text = "Looks good; PERSON_9 agreed.";
        let preview = app.restore_text(from.clone(), text.into()).unwrap();
        assert_eq!(preview.text, text);
        assert_eq!(preview.restorable, restorable);
        let reply = app
            .save_reply(from, "Reply".into(), None, text.into())
            .unwrap();
        assert_eq!(app.restore_reply(reply.id).unwrap(), preview);
    }
}
