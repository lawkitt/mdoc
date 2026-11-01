//! The recognition fixtures on their snapshots (ADR 0093), on any OS: each
//! fixture's committed raw model output through the correction stage and the
//! review, against its gold and status. The Mac's detection gate
//! (`just recognize --check`) also reruns the model to keep snapshots fresh.
use mdoc_detect::recognition::{Comparison, Fixture, Snapshot, fixture_paths, proposals, verdict};
use std::path::Path;

#[test]
fn every_recognition_fixture_is_as_its_status_says() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/recognition");
    let paths = fixture_paths(&root).unwrap();
    assert!(
        paths.len() >= 40,
        "fixtures missing under {}",
        root.display()
    );
    let config = mdoc_store::settings::PiiConfig::default();
    let model = mdoc_detect::model_name(mdoc_store::settings::PiiModel::Fp16);
    let mut failures = Vec::new();
    for path in &paths {
        let check = || -> Result<(), String> {
            let fixture = Fixture::read(path)?;
            fixture.validate()?;
            fixture.validate_layout()?;
            let snapshot = Snapshot::read(&fixture.snapshot_path())?;
            if snapshot.model != model || snapshot.threshold != config.threshold {
                return Err(format!(
                    "snapshot made with {} at {}, not {model} at {}; refresh it on the Mac",
                    snapshot.model, snapshot.threshold, config.threshold
                ));
            }
            snapshot.validate(&fixture.source)?;
            let proposed = proposals(&fixture.source, &snapshot.entities)?;
            let comparison = Comparison::new(&fixture.gold, &proposed);
            verdict(&fixture, &comparison).map_or(Ok(()), Err)
        };
        if let Err(error) = check() {
            failures.push(format!("{}: {error}", path.display()));
        }
    }
    // A snapshot whose fixture was renamed or removed.
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && let Some(id) = name.strip_suffix(".model.json")
                && !path.with_file_name(format!("{id}.md")).exists()
            {
                failures.push(format!("{}: snapshot without a fixture", path.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} recognition fixtures (just recognize <file> --gold --snapshot shows one):\n{}",
        failures.len(),
        paths.len(),
        failures.join("\n")
    );
}
