//! One ordering rule for Vault operations (ADR 0130). Random interleavings
//! of saves (keystrokes and names in the editor, notes, the reply box),
//! moves, Move to New Project, deletes (a Project, Unfiled documents),
//! Export Project and the quit, on gpui's seeded scheduler: after each run
//! no accepted edit is lost, every alias restores to the person it named,
//! nothing went that the user wasn't asked about, and each export holds
//! what the Vault stored when it ran. A failure prints its seed; `SEED=<n>`
//! runs that one again, and `ITERATIONS=<n>` runs more seeds. A guard test
//! keeps Vault writes inside the writer's jobs.
use super::*;
use mdoc_pii::{Category::Person, restore::RestoreMap};
use std::collections::{HashMap, HashSet};

/// The people the documents name.
const NAMES: [&str; 3] = ["Anna Petrova", "Boris Ivanov", "Clara Novak"];
/// Steps in one run.
const STEPS: usize = 30;

/// One document, as the user has edited it.
struct Doc {
    tab: u64,
    id: String,
    /// Its text with every name as typed: what its stored Markdown must
    /// restore to.
    plain: String,
    notes: Option<String>,
    reply: Option<String>,
}

/// An Export Project the run asked for: the folder chosen, and each
/// document's text as it stood then.
struct Asking {
    into: PathBuf,
    plain: HashMap<String, String>,
}

struct Run<'a> {
    tabs: Entity<Workspace>,
    cx: &'a mut VisualTestContext,
    vault: Arc<mdoc_vault::Vault>,
    docs: Vec<Doc>,
    /// Documents the user agreed to delete, as the questions named them.
    confirmed: HashSet<String>,
    /// A Project was deleted: its own replies went with it.
    project_deleted: bool,
    exports: Vec<Asking>,
    words: usize,
    folders: Vec<tempfile::TempDir>,
    /// What the run did, step by step, for a failure's message.
    log: Vec<String>,
}

impl Run<'_> {
    fn random(&mut self, below: usize) -> usize {
        self.cx.executor().rng().random_range(0..below)
    }

    /// Tab `tab`'s view, when the user could type in it now.
    fn ready(&mut self, tab: u64) -> Option<Entity<DocumentView>> {
        let tabs = self.tabs.clone();
        self.cx.update(|_, cx| {
            let view = tabs.read(cx).tabs.get(tab)?.view()?.clone();
            let ready = {
                let v = view.read(cx);
                v.lifecycle.ready()
                    && !v.lifecycle.source_only()
                    && !v.lifecycle.prompting()
                    && v.session.document.vault.is_some()
            };
            ready.then_some(view)
        })
    }

    fn type_word(&mut self, i: usize) {
        let Some(view) = self.ready(self.docs[i].tab) else {
            return;
        };
        self.words += 1;
        let word = format!(" w{}", self.words);
        view.update(self.cx, |view, cx| {
            view.editor.update(cx, |editor, cx| {
                let text = format!("{}{word}", editor.text());
                editor.set_text(text, cx);
            })
        });
        self.docs[i].plain.push_str(&word);
    }

    fn apply_name(&mut self, i: usize) {
        let Some(view) = self.ready(self.docs[i].tab) else {
            return;
        };
        let name = NAMES[self.random(NAMES.len())];
        view.update_in(self.cx, |view, window, cx| {
            let text = view.editor.read(cx).text().to_string();
            let Some(start) = text.find(name) else {
                return;
            };
            view.pii.set_reviewing(true);
            let marked = (view.pii.review_mut())
                .add_manual(&text, start..start + name.len(), Person)
                .is_ok();
            if marked {
                view.apply_all_pii(&PiiApplyAll, window, cx);
            }
        });
    }

    fn type_notes(&mut self, i: usize) {
        let Some(view) = self.ready(self.docs[i].tab) else {
            return;
        };
        self.words += 1;
        let notes = format!("Note {}.", self.words);
        view.update_in(self.cx, |view, window, cx| {
            view.type_notes(&notes, window, cx)
        });
        self.docs[i].notes = Some(notes);
    }

    fn fill_reply(&mut self, i: usize) {
        let Some(view) = self.ready(self.docs[i].tab) else {
            return;
        };
        self.words += 1;
        let reply = format!("Reply {}.", self.words);
        view.update_in(self.cx, |view, window, cx| {
            view.fill_reply(&reply, window, cx)
        });
        self.docs[i].reply = Some(reply);
    }

    fn projects(&self) -> Vec<String> {
        (self.vault.projects().into_iter())
            .map(|project| project.id)
            .collect()
    }

    fn move_doc(&mut self, i: usize) {
        let mut places: Vec<Option<String>> = self.projects().into_iter().map(Some).collect();
        places.push(None);
        let to = places.swap_remove(self.random(places.len()));
        let tab = self.docs[i].tab;
        self.tabs.update_in(self.cx, |tabs, window, cx| {
            tabs.move_tab(tab, to, window, cx)
        });
    }

    fn move_to_new(&mut self, i: usize) {
        let tab = self.docs[i].tab;
        self.tabs.update_in(self.cx, |tabs, window, cx| {
            tabs.move_to_new_project(tab, window, cx)
        });
    }

    fn delete_project(&mut self) {
        let projects = self.projects();
        if projects.is_empty() {
            return;
        }
        let project = projects[self.random(projects.len())].clone();
        self.tabs.update_in(self.cx, |tabs, window, cx| {
            tabs.delete_project(project, window, cx)
        });
    }

    fn delete_unfiled(&mut self) {
        let unfiled: Vec<String> = (self.vault.documents_in(None).into_iter())
            .map(|summary| summary.id)
            .collect();
        if unfiled.is_empty() {
            return;
        }
        let id = unfiled[self.random(unfiled.len())].clone();
        self.tabs.update_in(self.cx, |tabs, window, cx| {
            tabs.delete_documents(vec![id], window, cx)
        });
    }

    /// Answer the first delete question up as the user might, or Cancel.
    fn answer(&mut self, cancel: bool) {
        let Some((message, detail)) = self.cx.pending_prompt() else {
            return;
        };
        // What the question names, as the window asked it.
        let tabs = self.tabs.clone();
        let named: Vec<String> = self.cx.update(|_, cx| {
            tabs.update(cx, |tabs, _| tabs.projects.asked_to_delete.pop_front())
                .unwrap_or_default()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect()
        });
        let project = message.starts_with("Delete the Project");
        let answers: &[&str] = if detail.contains("can move to Unfiled") {
            &["Move to Unfiled", "Delete Documents", "Cancel"]
        } else {
            &["Delete", "Cancel"]
        };
        let answer = if cancel {
            "Cancel"
        } else {
            answers[self.random(answers.len())]
        };
        if answer != "Cancel" && project {
            self.project_deleted = true;
        }
        self.log.push(format!("answer {answer}"));
        if answer == "Delete" || answer == "Delete Documents" {
            self.confirmed.extend(named);
        }
        self.cx.simulate_prompt_answer(answer);
    }

    fn export(&mut self) {
        let projects = self.projects();
        if projects.is_empty() {
            return;
        }
        let project = projects[self.random(projects.len())].clone();
        self.tabs.update_in(self.cx, |tabs, window, cx| {
            tabs.export_project(project, window, cx)
        });
        if !self.cx.did_prompt_for_paths() {
            return;
        }
        let folder = tempfile::tempdir().unwrap();
        let into = folder.path().to_path_buf();
        let chosen = into.clone();
        self.cx
            .simulate_path_prompt_response(move |_| Some(vec![chosen]));
        let plain = (self.docs.iter())
            .map(|doc| (doc.id.clone(), doc.plain.clone()))
            .collect();
        self.exports.push(Asking { into, plain });
        self.folders.push(folder);
    }

    /// Let the scheduler run some of what is due, in its seeded order, and
    /// sometimes let time pass (typing pauses, the fold's).
    fn ticks(&mut self) {
        for _ in 0..self.random(8) {
            if !self.cx.executor().tick() {
                break;
            }
        }
        if self.random(4) == 0 {
            let millis = [20, 60, 300, 800][self.random(4)];
            self.cx
                .executor()
                .advance_clock(std::time::Duration::from_millis(millis));
        }
    }

    /// The window is still open: the quit has not closed it.
    fn open(&self) -> bool {
        self.cx.cx.read(|cx| !cx.windows().is_empty())
    }

    fn step(&mut self) {
        if !self.open() {
            return;
        }
        if self.cx.has_pending_prompt() && self.random(3) == 0 {
            self.answer(false);
            return;
        }
        let i = self.random(self.docs.len());
        let questioned = self.cx.has_pending_prompt();
        let roll = self.random(100);
        let doc = ["a", "b", "c"][i];
        self.log.push(match roll {
            0..30 => format!("type {doc}"),
            30..40 => format!("name {doc}"),
            40..46 => format!("notes {doc}"),
            46..51 => format!("reply {doc}"),
            51..60 => format!("move {doc}"),
            60..68 => format!("type+move {doc}"),
            68..72 => format!("new project {doc}"),
            72..77 if !questioned => "delete project".into(),
            77..81 if !questioned => "delete unfiled".into(),
            81..86 => "export".into(),
            _ => "wait".into(),
        });
        match roll {
            0..30 => self.type_word(i),
            30..40 => self.apply_name(i),
            40..46 => self.type_notes(i),
            46..51 => self.fill_reply(i),
            51..60 => self.move_doc(i),
            // A keystroke and a move at once: the move reads it at its head.
            60..68 => {
                self.type_word(i);
                self.move_doc(i);
            }
            68..72 => self.move_to_new(i),
            72..77 if !questioned => self.delete_project(),
            77..81 if !questioned => self.delete_unfiled(),
            81..86 => self.export(),
            _ => {}
        }
        self.ticks();
    }

    /// Quit at this point of the run, whatever is still queued, running or
    /// asking; editing goes on until the quit's turn closes the window.
    fn quit(&mut self) {
        if self.open() {
            self.log.push("quit".into());
            self.cx.dispatch_action(Quit);
        }
    }

    /// End the run: questions left are cancelled, everything due runs, and
    /// the window has quit.
    fn finish(&mut self) -> Vec<crate::workspace::projects::export_project::ExportSeen> {
        for _ in 0..4 {
            if !self.open() {
                break;
            }
            while self.cx.has_pending_prompt() {
                self.answer(true);
            }
            self.quit();
            self.cx
                .executor()
                .advance_clock(std::time::Duration::from_secs(2));
            self.cx.run_until_parked();
        }
        if self.open() {
            let tabs = self.tabs.clone();
            let notice =
                (self.cx).update(|_, cx| tabs.read(cx).notices.message().map(str::to_owned));
            panic!("the window did not quit: {notice:?}");
        }
        let tabs = self.tabs.clone();
        (self.cx.cx).update(|cx| {
            tabs.update(cx, |tabs, _| {
                std::mem::take(&mut tabs.projects.exports_seen)
            })
        })
    }
}

/// Open a document of `text` as `name`, its `names` applied.
fn document(
    tabs: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    dir: &std::path::Path,
    name: &str,
    text: &str,
    names: &[&str],
) -> Doc {
    let names: Vec<_> = names.iter().map(|name| (*name, Person)).collect();
    let tab = open_applied(tabs, cx, dir, name, text, &names);
    let id = cx.update(|_, cx| tabs.read(cx).tab_document(tab, cx).unwrap());
    Doc {
        tab,
        id: id.as_str().to_string(),
        plain: text.to_string(),
        notes: None,
        reply: None,
    }
}

/// One run from the scheduler's seed; `broken`: moves in the order the rule
/// forbids.
fn interleave(cx: &mut TestAppContext, broken: bool) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("vault");
    let vault_root = root.clone();
    let (tabs, cx) = boot_home_with(cx, Session::default(), move |tabs, _, _| {
        tabs.use_vault(mdoc_vault::Vault::open(vault_root, &[3; 32]).unwrap());
    });
    tabs.update(cx, |tabs, _| tabs.broken_order = broken);
    // Two Projects whose PERSON_1 are different people, and an Unfiled
    // document naming both.
    let a = document(
        &tabs,
        cx,
        dir.path(),
        "a.md",
        "Anna Petrova wrote to Boris Ivanov.",
        &["Anna Petrova"],
    );
    new_project(&tabs, cx, a.tab);
    let b = document(
        &tabs,
        cx,
        dir.path(),
        "b.md",
        "Boris Ivanov signed for Clara Novak.",
        &["Boris Ivanov"],
    );
    new_project(&tabs, cx, b.tab);
    let c = document(
        &tabs,
        cx,
        dir.path(),
        "c.md",
        "Clara Novak met Anna Petrova.",
        &["Clara Novak"],
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    let vault = vault(&tabs, cx);
    let mut run = Run {
        tabs,
        cx,
        vault,
        docs: vec![a, b, c],
        confirmed: HashSet::new(),
        project_deleted: false,
        exports: Vec::new(),
        words: 0,
        folders: Vec::new(),
        log: Vec::new(),
    };
    // The quit comes at a seeded step, with work still queued; the steps
    // after it type on until the window closes.
    let quit_at = STEPS / 2 + run.random(STEPS / 2);
    for step in 0..STEPS {
        if step == quit_at {
            run.quit();
        }
        run.step();
    }
    let seen = run.finish();
    check(&run, &root, &seen);
}

/// What must hold after a run, read from a new handle on the Vault.
fn check(
    run: &Run,
    root: &std::path::Path,
    seen: &[crate::workspace::projects::export_project::ExportSeen],
) {
    let vault = mdoc_vault::Vault::open(root, &[3; 32]).unwrap();
    let names_of = |document: &mdoc_vault::Document| match &document.project {
        Some(project) => vault.project(project).unwrap().mapping.exported(),
        None => document.mapping.exported(),
    };
    let replies: Vec<String> = (vault.replies().into_iter())
        .filter_map(|reply| vault.reply(&reply.id).ok())
        .map(|reply| reply.markdown)
        .collect();
    for doc in &run.docs {
        let stored = match vault.document(&doc.id) {
            Ok(stored) => stored,
            Err(_) if run.confirmed.contains(&doc.id) => continue,
            Err(error) => panic!("{} went unasked: {error}", doc.id),
        };
        let restored = RestoreMap::new(names_of(&stored)).restore_in_place(&stored.markdown);
        assert_eq!(
            restored.text,
            doc.plain,
            "lost an edit or restored an alias to another person: {:?} after {}",
            stored.markdown,
            run.log.join(", ")
        );
        if let Some(notes) = &doc.notes {
            assert_eq!(&stored.notes, notes, "lost notes");
        }
        if let Some(reply) = &doc.reply
            && !run.project_deleted
        {
            assert!(
                replies.contains(reply),
                "lost the reply {reply:?}: {replies:?}"
            );
        }
    }
    for export in seen {
        let asked = (run.exports.iter())
            .find(|asking| export.folder.parent() == Some(asking.into.as_path()))
            .expect("an export the run asked for");
        let map = RestoreMap::new(export.names.clone());
        for (id, title, markdown, notes) in &export.documents {
            let read = |extension: &str| {
                std::fs::read_to_string(export.folder.join(format!("{title}{extension}")))
            };
            assert_eq!(
                &read(".md").unwrap(),
                markdown,
                "the export is what was stored"
            );
            if !notes.trim().is_empty() {
                assert_eq!(&read(".notes.md").unwrap(), notes);
            }
            if let Some(plain) = asked.plain.get(id) {
                let restored = map.restore_in_place(markdown).text;
                assert!(
                    restored.starts_with(plain.as_str()),
                    "the export missed an edit made before it was asked for: {restored:?} \
                     is not {plain:?}"
                );
            }
        }
    }
}

/// ADR 0130: in any interleaving, nothing accepted is lost, no alias names
/// someone else, nothing goes unasked, and exports are what was stored.
#[gpui::test(iterations = 12)]
fn any_interleaving_of_vault_operations_keeps_every_edit_and_name(cx: &mut TestAppContext) {
    interleave(cx, false);
}

/// The proof that the interleavings test something: with each move reading
/// its tab after it read the Vault, the order ADR 0130 forbids, some run
/// restores an alias to another person.
#[gpui::test(iterations = 12)]
#[should_panic(expected = "restored an alias to another person")]
fn the_interleavings_catch_a_move_that_reads_its_tab_too_late(cx: &mut TestAppContext) {
    interleave(cx, true);
}

/// The Vault's write methods, and `export_project`: a read, but of one
/// committed state, so it runs as a job too.
const WRITES: &[&str] = &[
    "create_project",
    "create_project_with",
    "update_project",
    "save_project",
    "create_document",
    "import_document",
    "import_document_expecting",
    "publish_import",
    "update_document",
    "set_notes",
    "save_document",
    "delete_document",
    "delete_unfiled_documents",
    "update_user_lists",
    "save_user_lists",
    "move_document",
    "move_document_to_new_project",
    "move_document_to_new_project_with",
    "undo_move",
    "delete_project",
    "delete_project_with",
    "save_reply",
    "save_shared_reply",
    "update_shared_reply",
    "promote_reply",
    "delete_reply",
    "flush",
    "export_project",
];

/// Receivers that are the window itself, whose methods of the same names
/// queue jobs (`self.move_document(…)`), and files' own `flush`.
const NOT_THE_VAULT: &[&str] = &["self", "this", "tabs", "file", "pipe", "out", "stdout"];

/// The job bodies, by file under `src/` and function.
const JOBS: &[(&str, &str)] = &[
    ("workspace/saving/quit_flow.rs", "run_quit"),
    ("workspace/saving/autosave.rs", "run_notes_save"),
    ("workspace/saving/autosave.rs", "run_save"),
    ("workspace/saving/autosave.rs", "write_captured"),
    ("workspace/saving/autosave.rs", "save_vault"),
    ("workspace/saving/autosave.rs", "create_tab_now"),
    ("workspace/projects/actions.rs", "write_now"),
    ("workspace/projects/moves/actions.rs", "run_op"),
    ("workspace/projects/delete.rs", "run_delete"),
    ("workspace/sidebar/actions.rs", "run_delete_documents"),
    ("workspace/replies.rs", "run_reply_save"),
    ("workspace/replies.rs", "run_promote"),
    ("workspace/replies.rs", "run_delete_reply"),
    ("workspace/saving/intake.rs", "run_import"),
    (
        "workspace/projects/folder_sheet/actions.rs",
        "run_new_project",
    ),
    (
        "workspace/projects/new_project/actions.rs",
        "run_empty_project",
    ),
    ("workspace/vault/user_lists.rs", "run_user_lists"),
    ("workspace/projects/export_project.rs", "run_export"),
];

/// Every call of a Vault write method in `text`, whatever its receiver is
/// named (`vault.…`, `store.…`, `self.vault.vault()?.…`), but the window's
/// own: the function it is in, and the method.
fn vault_writes(text: &str) -> Vec<(String, String)> {
    let call = regex::Regex::new(&format!(
        r"\b(\w+)(?:\(\))?\??\s*\.\s*({})\s*\(",
        WRITES.join("|")
    ))
    .unwrap();
    let function = regex::Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)").unwrap();
    (call.captures_iter(text))
        .filter(|at| !NOT_THE_VAULT.contains(&&at[1]))
        .map(|at| {
            let start = at.get(0).unwrap().start();
            let within = (function.captures_iter(&text[..start]).last())
                .map(|function| function[1].to_string())
                .unwrap_or_default();
            (within, at[2].to_string())
        })
        .collect()
}

/// The scan finds a write however its receiver is named, and leaves the
/// window's own methods of the same names alone.
#[test]
fn the_writer_guard_finds_a_vault_write_through_any_name() {
    let text = "fn run_op() {\n    vault.move_document(id, None);\n}\n\
                fn sneaky(store: &Vault) {\n    store.update_document(id, |_| {});\n    \
                self.vault.vault()?.set_notes(id, notes);\n    \
                self.move_document(id, None, window, cx);\n    file.flush();\n}\n";
    assert_eq!(
        vault_writes(text),
        [
            ("run_op".to_string(), "move_document".to_string()),
            ("sneaky".to_string(), "update_document".to_string()),
            ("sneaky".to_string(), "set_notes".to_string()),
        ]
    );
}

/// ADR 0130, rule 1: the window writes the Vault only from a job at the
/// head of its writer. Every call of a Vault write method in the app's own
/// code is in one of the job bodies in `JOBS`; a write anywhere else fails
/// this, before it can race the queue.
#[test]
fn the_vault_is_written_only_by_jobs_of_the_writer() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                // Tests and the render scenes set up their own Vaults.
                if name != "tests" && name != "headless_renders" {
                    dirs.push(path);
                }
                continue;
            }
            // `headless_renders.rs`: the dev-only render scenes, as above.
            if !name.ends_with(".rs") || name == "tests.rs" || name == "headless_renders.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap()
                .replace("\r\n", "\n");
            // A file's own unit tests come last.
            let text = text.split("\n#[cfg(test)]\nmod tests").next().unwrap();
            let file = (path.strip_prefix(&root).unwrap().to_string_lossy()).replace('\\', "/");
            for (within, method) in vault_writes(text) {
                found.push((file.clone(), within, method));
            }
        }
    }
    let outside: Vec<_> = (found.iter())
        .filter(|(file, within, _)| !JOBS.contains(&(file.as_str(), within.as_str())))
        .collect();
    assert!(
        outside.is_empty(),
        "Vault writes outside the writer's jobs (ADR 0130): {outside:?}"
    );
    // The scan sees the writes it is meant to: one in each job body.
    for job in JOBS {
        assert!(
            found
                .iter()
                .any(|(file, within, _)| (file.as_str(), within.as_str()) == *job),
            "no Vault write found in {job:?}: the list is stale"
        );
    }
}
