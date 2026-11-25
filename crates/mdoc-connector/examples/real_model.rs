//! The Vault side of `scripts/dev/connector-real-model.sh`, run with
//! `MDOC_DEV_PROFILE` naming a new scratch folder (debug builds on macOS
//! and Windows only):
//!
//! - `seed`: settings with "AI connector (preview)" on, and a Vault holding
//!   one Project whose Ready lease names its people only by alias. Prints
//!   the Project's id, which mdoc then shares.
//! - `check <transcript>`: after the run, whether the Vault recorded the
//!   read and the delivery, the reply restores to the real names, and no
//!   real name appears in the transcript (stream-json) or the reply as
//!   delivered. Prints a line per check; fails when one does.
//!
//! It never reads the real profile: without a dev profile it stops.
use mdoc_pii::{Category, restore::ExportedIdentity};
use mdoc_store::{app_paths, os_key, settings};
use mdoc_vault::{AiEvent, DocumentOrigin, NewDocument, NewProject, SourceKind, Vault};
use serde_json::Value;
use std::{path::Path, process::ExitCode, sync::Arc};

/// The people behind the aliases. None may leave mdoc.
const PEOPLE: [(&str, &str); 2] = [("PERSON_1", "Anna Petrova"), ("PERSON_2", "Boris Ivanov")];

const LEASE: &str = "# Lease\n\n\
PERSON_1 rents the flat at 12 Garden Street from PERSON_2 for 900 EUR a month, \
from 1 March 2026. PERSON_1 pays a deposit of 1,800 EUR. PERSON_2 repairs the \
heating within 14 days of being told it is broken.\n\n\
Signed by PERSON_1 and PERSON_2 on 14 February 2026.\n";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["seed"] => seed(),
        ["check", transcript] => check(Path::new(transcript)),
        _ => Err("usage: real_model seed | real_model check <transcript>".into()),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("real_model: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The dev profile's Vault; an error without a dev profile, so the real
/// one is never opened.
fn vault() -> Result<Vault, String> {
    if app_paths::use_dev_profile().is_none() {
        return Err("set MDOC_DEV_PROFILE to a new folder (debug builds only)".into());
    }
    let key = os_key::vault_key(false)
        .map_err(|e| e.to_string())?
        .ok_or("no Vault key")?;
    let root = os_key::vault_dir().ok_or("no Vault folder")?;
    Vault::open(root, &key).map_err(|e| e.to_string())
}

fn seed() -> Result<bool, String> {
    let vault = vault()?;
    if !vault.projects().is_empty() {
        return Err("the dev profile already holds a Project: use a new one".into());
    }
    let mut preferences = settings::Preferences::default();
    preferences.ai_connector.preview = true;
    let path = settings::Store::default_path().ok_or("no settings folder")?;
    settings::save(&path, &preferences).map_err(|e| e.to_string())?;
    let people: Vec<ExportedIdentity> = (PEOPLE.iter())
        .map(|(alias, original)| ExportedIdentity {
            alias: (*alias).into(),
            previous: Vec::new(),
            category: Category::Person,
            custom: false,
            originals: vec![Arc::from(*original)],
        })
        .collect();
    let project = (vault.create_project(NewProject {
        name: "Lease dispute".into(),
        ..NewProject::default()
    }))
    .map_err(|e| e.to_string())?;
    (vault.update_project(&project.id, |p| p.mapping.merge(&people))).map_err(|e| e.to_string())?;
    let document = (vault.create_document(NewDocument {
        project: Some(project.id.clone()),
        title: "Lease".into(),
        source_kind: SourceKind::Text,
        origin: DocumentOrigin::Ordinary,
        markdown: LEASE.into(),
    }))
    .map_err(|e| e.to_string())?;
    (vault.update_document(&document.id, |d| {
        d.scanned = true;
        d.set_ready(true);
    }))
    .map_err(|e| e.to_string())?;
    println!("{}", project.id);
    Ok(true)
}

/// Each real name's words, lowercased: none may appear anywhere.
fn real_words() -> Vec<String> {
    (PEOPLE.iter())
        .flat_map(|(_, original)| original.split_whitespace())
        .map(str::to_lowercase)
        .collect()
}

fn leaks(text: &str) -> Vec<String> {
    let text = text.to_lowercase();
    (real_words().into_iter())
        .filter(|word| text.contains(word))
        .collect()
}

fn check(transcript: &Path) -> Result<bool, String> {
    let vault = vault()?;
    let projects = vault.projects();
    let [project] = &projects[..] else {
        return Err("the dev profile should hold the one seeded Project".into());
    };
    let documents = vault.documents_in(Some(&project.id));
    let [document] = &documents[..] else {
        return Err("the Project should hold the one seeded lease".into());
    };
    let transcript = std::fs::read_to_string(transcript).map_err(|e| e.to_string())?;
    let mut ok = true;
    let mut report = |passed: bool, what: &str| {
        println!("{} {what}", if passed { "PASS" } else { "FAIL" });
        ok &= passed;
    };

    // What the model did, from the transcript's tool calls and text.
    let mut tools = Vec::new();
    let mut texts = Vec::new();
    for line in transcript.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event["type"] != "assistant" {
            continue;
        }
        for block in event["message"]["content"].as_array().into_iter().flatten() {
            match block["type"].as_str() {
                Some("tool_use") => tools.push(block["name"].as_str().unwrap_or("").to_string()),
                Some("text") => texts.push(block["text"].as_str().unwrap_or("").to_string()),
                _ => {}
            }
        }
    }
    let called = |tool: &str| tools.iter().any(|t| t == &format!("mcp__mdoc__{tool}"));
    report(
        called("list_documents"),
        "the model listed the shared documents",
    );
    report(called("get_document"), "the model read a document");
    report(
        texts.iter().any(|t| t.contains("PERSON_")),
        "the model showed its answer with aliases",
    );
    report(called("deliver_reply"), "the model delivered its answer");

    let activity = vault.ai_activity(&project.id).map_err(|e| e.to_string())?;
    report(
        (activity.iter())
            .any(|a| matches!(&a.event, AiEvent::Read { document: d, .. } if *d == document.id)),
        "the Vault recorded the read",
    );
    let delivered = activity.iter().find_map(|a| match &a.event {
        AiEvent::Delivered { reply, read, .. } if read.contains(&document.id) => Some(reply),
        _ => None,
    });
    report(delivered.is_some(), "the Vault recorded the delivery");

    if let Some(reply) = delivered {
        let received = vault.reply(reply).map_err(|e| e.to_string())?.markdown;
        let restored = vault.restore_reply(reply).map_err(|e| e.to_string())?.text;
        report(
            received.contains("PERSON_") && leaks(&received).is_empty(),
            "the reply arrived with aliases and no real name",
        );
        report(
            PEOPLE
                .iter()
                .any(|(_, original)| restored.contains(original)),
            "the reply restores to real names in mdoc",
        );
    }
    let leaked = leaks(&transcript);
    report(
        leaked.is_empty(),
        &format!("no real name in the transcript (found: {leaked:?})"),
    );
    Ok(ok)
}
